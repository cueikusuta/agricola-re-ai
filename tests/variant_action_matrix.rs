use agricola_re_ai::engine::{Action, AnimalPlan, Detail};
use agricola_re_ai::model::{Animal, Config, Game, Space};

fn game(additional: bool) -> Game {
    Game::new(Config {
        seed: 7205,
        additional_action_space_enabled: additional,
    })
}

#[test]
fn two_player_animal_market_has_three_distinct_transactions() {
    for (kind, delta_food) in [
        (Animal::Sheep, 1i32),
        (Animal::Boar, 0i32),
        (Animal::Cattle, -1i32),
    ] {
        let mut game = game(true);
        let player = game.public.active_player;
        let food_before = game.public.players[player].supply.food as i32;
        assert!(game.legal_spaces().contains(&Space::AnimalMarket));
        game.apply(Action {
            space: Space::AnimalMarket,
            detail: Detail::Animals(AnimalPlan::discard_all(kind, 1)),
        })
        .unwrap();
        assert_eq!(
            game.public.players[player].supply.food as i32,
            food_before + delta_food
        );
        assert_eq!(game.public.players[player].animal_count(kind), 0);
        assert!(!game.legal_spaces().contains(&Space::ResourceMarket));
        assert!(!game.legal_spaces().contains(&Space::Copse));
    }
}

#[test]
fn resource_market_provides_stone_and_food_not_reed() {
    let mut game = game(true);
    let player = game.public.active_player;
    let before = game.public.players[player].supply.clone();
    game.apply(Action {
        space: Space::ResourceMarket,
        detail: Detail::None,
    })
    .unwrap();
    let after = &game.public.players[player].supply;
    assert_eq!(after.stone, before.stone + 1);
    assert_eq!(after.food, before.food + 1);
    assert_eq!(after.reed, before.reed);
}

#[test]
fn disabled_additional_tile_never_exposes_variant_actions() {
    let game = game(false);
    for action in [
        Space::AnimalMarket,
        Space::ResourceMarket,
        Space::Copse,
        Space::ModestFamilyGrowth,
    ] {
        assert!(!game.legal_spaces().contains(&action));
    }
}

#[test]
fn beginner_meeting_place_gains_accumulated_food_and_claims_start_player() {
    let mut game = game(false);
    let player = game.public.active_player;
    let start_food = game.public.players[player].supply.food;
    assert_eq!(game.public.accumulations.get(&Space::MeetingPlace), Some(&1));
    game.apply(Action {
        space: Space::MeetingPlace,
        detail: Detail::None,
    })
    .unwrap();
    assert_eq!(game.public.start_player, player);
    assert_eq!(game.public.players[player].supply.food, start_food + 1);
    assert!(!game.legal_spaces().contains(&Space::MeetingPlace));
}
