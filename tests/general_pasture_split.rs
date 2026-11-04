use agricola_re_ai::engine::{Action, Detail};
use agricola_re_ai::model::{Config, Game, RoundCard, Space, Tile};
use agricola_re_ai::policy::{DetailPolicy, SimpleDetails};

fn bridged_pasture(enabled: bool) -> Game {
    let mut game = Game::new(Config {
        seed: 4971,
        additional_action_space_enabled: enabled,
    });
    if game.public.revealed[0] != RoundCard::Fencing {
        let index = game.hidden_round_deck.iter()
            .position(|&c| c == RoundCard::Fencing).unwrap();
        let original = game.public.revealed[0];
        game.public.revealed[0] = RoundCard::Fencing;
        game.hidden_round_deck[index] = original;
    }
    let id = game.public.active_player;
    game.public.players[id].supply.wood = 14;
    game.apply(Action {
        space: Space::Fencing,
        detail: Detail::Fence {
            pastures: vec![vec![1, 2, 6, 7, 8, 9, 13, 14]],
        },
    }).unwrap();
    assert_eq!(game.public.players[id].fences.len(), 14);

    // The only possible extra fence is a bridge between two non-singleton
    // components. Each singleton isolation would need at least two fences.
    let player = &mut game.public.players[id];
    for cell in &mut player.farm {
        if matches!(cell.tile, Tile::Empty) {
            cell.tile = Tile::Field { crop: None, remaining: 0 };
        }
    }
    player.supply.wood = 1;
    game.public.used.clear();
    game.public.workers_placed[id] = 0;
    game.validate().unwrap();
    game
}

#[test]
fn one_fence_can_split_two_connected_multi_cell_areas() {
    for enabled in [false, true] {
        let mut game = bridged_pasture(enabled);
        let id = game.public.active_player;
        assert!(game.legal_spaces().contains(&Space::Fencing));
        let obs = game.observation(id).unwrap();
        let decision = SimpleDetails.produce(&obs, Space::Fencing).unwrap();
        assert!(matches!(decision.detail, Detail::FenceDetailed { .. }));
        game.apply(decision).unwrap();
        assert_eq!(game.public.players[id].pastures.len(), 2);
        assert_eq!(game.public.players[id].fences.len(), 15);
        assert_eq!(game.public.players[id].supply.wood, 0);
    }
}

#[test]
fn the_same_split_is_unavailable_with_no_wood() {
    let mut game = bridged_pasture(false);
    let actor = game.public.active_player;
    game.public.players[actor].supply.wood = 0;
    assert!(!game.legal_spaces().contains(&Space::Fencing));
}
