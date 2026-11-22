//! B102 女性顧問 (Consultant): the per-player-count on-play bonus —
//! 2 players 3 clay, 3 players 2 reed, 4 players 2 sheep.
//!
//! The 4-player sheep series reuses the existing `OnPlayGainAnimal` on-play
//! auto-placement path (C083 若牛's vocabulary): `engine::place_animals`
//! derives an `AnimalPlan` from the board (free pasture/stable capacity
//! first, then the pet slot, overflow discarded) and applies it without a
//! play-time pause. No new `CardEffect` variant, `PendingKind` or save field
//! is introduced, so the v5 hash material is untouched.
//!
//! Every play here goes through the canonical engine route
//! (`Game::apply` -> `cards::play_validate_pay` -> zone move ->
//! `card_events::dispatch_play` -> `handle_on_play_gain_animal`), not the
//! `cards::apply_on_play_effects` shortcut that only knows `Good` gains.

use agricola_re_ai::cards::{card_set_for_game, CardKind};
use agricola_re_ai::engine::{Action, Detail};
use agricola_re_ai::model::{Animal, Config, Game, Space};

/// Explicit 7+7 hands for `players` seats with B102 seeded into seat 0.
/// Cards are drawn from the experimental pool for the player count, which
/// doubles as the registry/pool acceptance check (B102 must be eligible at
/// that count for a 7-per-seat hand to exist).
fn hands_with_b102_in_seat0(players: usize) -> Vec<Vec<String>> {
    let set = card_set_for_game(players, true);
    assert!(
        set.definitions.iter().any(|card| card.id == "B102"),
        "the {players}-player pool must accept B102"
    );
    let mut occupations: Vec<String> = set
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
    assert!(
        occupations.len() >= 7 * players,
        "pool needs 7*{players} occupations, has {}",
        occupations.len()
    );
    assert!(
        minors.len() >= 7 * players,
        "pool needs 7*{players} minor improvements, has {}",
        minors.len()
    );
    // B102 first in the occupation pool so seat 0's first occupation is B102.
    occupations.retain(|id| id != "B102");
    let mut occ_pool = vec!["B102".to_string()];
    occ_pool.extend(occupations);

    let mut hands: Vec<Vec<String>> = vec![Vec::new(); players];
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

/// Start a game with B102 in seat 0's hand and seat 0 as the start player.
fn game_with_b102_seat0(players: usize) -> Game {
    let config = Config {
        players,
        seed: 7,
        ..Config::default()
    };
    let set = card_set_for_game(players, true);
    Game::with_cards_and_hands_with_start_player(config, set, hands_with_b102_in_seat0(players), 0)
        .expect("game setup with B102 in seat 0's hand")
}

/// Play seat 0's B102 through the Lessons action (the canonical engine route).
fn play_b102_from_seat0(game: &mut Game) {
    assert_eq!(game.public.active_player, 0, "seat 0 must start");
    game.apply(Action {
        space: Space::Lessons,
        detail: Detail::PlayOccupation {
            card_id: "B102".to_string(),
        },
    })
    .expect("B102 play through the engine route");
}

#[test]
fn pool_accepts_b102_at_two_three_and_four_players() {
    for players in [2, 3, 4] {
        let set = card_set_for_game(players, true);
        let b102 = set
            .definitions
            .iter()
            .find(|card| card.id == "B102")
            .unwrap_or_else(|| panic!("the {players}-player pool must contain B102"));
        let on_play_gains = b102
            .effects
            .iter()
            .filter(|effect| {
                matches!(
                    effect,
                    agricola_re_ai::cards::CardEffect::OnPlayGain { .. }
                        | agricola_re_ai::cards::CardEffect::OnPlayGainAnimal { .. }
                )
            })
            .count();
        assert_eq!(
            on_play_gains, 1,
            "{players} players: exactly one on-play series must project"
        );
    }
}

#[test]
fn four_players_play_places_two_sheep_via_auto_placement() {
    let mut game = game_with_b102_seat0(4);
    // Two empty stables give the auto-placement plan room for both sheep
    // (stable sites are filled before the pet slot / discard).
    game.public.players[0].farm[1].stable = true;
    game.public.players[0].farm[2].stable = true;
    assert_eq!(game.public.players[0].animal_count(Animal::Sheep), 0);
    let clay_before = game.public.players[0].supply.clay;
    let reed_before = game.public.players[0].supply.reed;

    play_b102_from_seat0(&mut game);

    let owner = &game.public.players[0];
    assert_eq!(
        owner.animal_count(Animal::Sheep),
        2,
        "4 players: B102 must place 2 sheep"
    );
    assert_eq!(
        owner.farm[1].stable_animal,
        Some(Animal::Sheep),
        "first sheep takes the first empty stable"
    );
    assert_eq!(
        owner.farm[2].stable_animal,
        Some(Animal::Sheep),
        "second sheep takes the second empty stable"
    );
    // The 2/3-player series must not fire alongside the 4-player one.
    assert_eq!(owner.supply.clay, clay_before, "4 players: no clay series");
    assert_eq!(owner.supply.reed, reed_before, "4 players: no reed series");
    for seat in 1..4 {
        assert_eq!(
            game.public.players[seat].animal_count(Animal::Sheep),
            0,
            "sheep must not leak to seat {seat}"
        );
    }
    assert!(
        game.public.players[0]
            .played_cards
            .iter()
            .any(|id| id == "B102"),
        "B102 must reach the played area"
    );
}

#[test]
fn four_players_without_room_keeps_one_sheep_as_pet() {
    let mut game = game_with_b102_seat0(4);
    // No pasture, no stable: the auto plan puts one sheep in the pet slot and
    // discards the overflow (pinned by the C083 on-play path comment).
    play_b102_from_seat0(&mut game);

    let owner = &game.public.players[0];
    assert_eq!(owner.animal_count(Animal::Sheep), 1);
    assert_eq!(owner.pet, Some(Animal::Sheep));
    assert!(
        owner.pastures.is_empty(),
        "no pasture may be created implicitly"
    );
}

#[test]
fn two_players_play_grants_clay_and_no_sheep() {
    let mut game = game_with_b102_seat0(2);
    let clay_before = game.public.players[0].supply.clay;
    let reed_before = game.public.players[0].supply.reed;

    play_b102_from_seat0(&mut game);

    let owner = &game.public.players[0];
    assert_eq!(owner.supply.clay - clay_before, 3, "2 players: clay 3");
    assert_eq!(owner.supply.reed, reed_before, "2 players: no reed");
    assert_eq!(owner.animal_count(Animal::Sheep), 0, "2 players: no sheep");
}

#[test]
fn three_players_play_grants_reed_and_no_sheep() {
    let mut game = game_with_b102_seat0(3);
    let clay_before = game.public.players[0].supply.clay;
    let reed_before = game.public.players[0].supply.reed;

    play_b102_from_seat0(&mut game);

    let owner = &game.public.players[0];
    assert_eq!(owner.supply.reed - reed_before, 2, "3 players: reed 2");
    assert_eq!(owner.supply.clay, clay_before, "3 players: no clay");
    assert_eq!(owner.animal_count(Animal::Sheep), 0, "3 players: no sheep");
}
