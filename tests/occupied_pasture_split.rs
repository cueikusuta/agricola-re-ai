use agricola_re_ai::engine::{Action, Detail};
use agricola_re_ai::model::{Animal, Config, Game, RoundCard, Space};
use agricola_re_ai::policy::{DetailPolicy, SimpleDetails};

fn reveal_fencing(game: &mut Game) {
    if game.public.revealed.contains(&RoundCard::Fencing) {
        return;
    }
    let index = game
        .hidden_round_deck
        .iter()
        .position(|&card| card == RoundCard::Fencing)
        .unwrap();
    let replaced = game.public.revealed[0];
    game.public.revealed[0] = RoundCard::Fencing;
    game.hidden_round_deck[index] = replaced;
}

fn surrounded_occupied_pasture(enabled: bool) -> Game {
    let config = Config { players: 2,
        seed: 441,
        additional_action_space_enabled: enabled,
    };
    let mut staging = Game::new(config.clone());
    reveal_fencing(&mut staging);
    let id = staging.public.active_player;
    staging.public.players[id].supply.wood = 7;
    staging
        .apply(Action {
            space: Space::Fencing,
            detail: Detail::Fence {
                pastures: vec![vec![13, 14]],
            },
        })
        .unwrap();

    let mut game = Game::new(config);
    reveal_fencing(&mut game);
    let id = game.public.active_player;
    let mut player = staging.public.players[id].clone();
    player.pastures[0].animal = Some(Animal::Sheep);
    player.pastures[0].quantity = 3;
    // With one wood left, no new one-cell pasture can be enclosed.
    game.public.players[id] = player;
    game.validate().unwrap();
    game
}

#[test]
fn occupied_split_is_legal_and_default_details_conserve_animals() {
    for enabled in [false, true] {
        let mut game = surrounded_occupied_pasture(enabled);
        let player = game.public.active_player;
        assert!(game.legal_spaces().contains(&Space::Fencing));
        let observed = game.observation(player).unwrap();
        let action = SimpleDetails.produce(&observed, Space::Fencing).unwrap();
        assert!(matches!(action.detail, Detail::FenceDetailed { .. }));
        game.apply(action).unwrap();
        assert_eq!(game.public.players[player].pastures.len(), 2);
        assert_eq!(game.public.players[player].animal_count(Animal::Sheep), 3);
        assert_eq!(game.public.players[player].fences.len(), 7);
    }
}

#[test]
fn occupied_split_is_not_legal_without_new_fence_resources() {
    let mut game = surrounded_occupied_pasture(false);
    let player = game.public.active_player;
    game.public.players[player].supply.wood = 0;
    assert!(!game.legal_spaces().contains(&Space::Fencing));
}
