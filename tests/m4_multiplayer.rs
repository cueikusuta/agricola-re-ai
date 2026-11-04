//! M4 multi-seat board configuration, optional tile and replay.
use agricola_re_ai::engine::{Action, Detail};
use agricola_re_ai::model::{Config, Good, Space};
use agricola_re_ai::policy::{play_to_end, DetailPolicy, GreedyPolicy, SimpleDetails};
use agricola_re_ai::Game;

fn config(players: usize, extra: bool) -> Config {
    Config {
        seed: 411,
        additional_action_space_enabled: extra,
        players,
    }
}

#[test]
fn board_spaces_match_player_count() {
    for players in 2..=4 {
        let game = Game::new(config(players, false));
        assert_eq!(game.public.players.len(), players);
        assert_eq!(game.public.work_quota, vec![2; players]);
        let legal = game.legal_spaces();
        assert_eq!(legal.contains(&Space::Grove), players >= 3);
        assert_eq!(legal.contains(&Space::Hollow), players >= 3);
        assert_eq!(legal.contains(&Space::TravelingPlayers), players == 4);
        assert_eq!(legal.contains(&Space::Copse), players == 4);
        assert_eq!(legal.contains(&Space::ResourceMarket), players >= 3);
        assert!(!legal.contains(&Space::AnimalMarket));
        game.validate().unwrap();
    }
}

#[test]
fn optional_tile_is_mutually_exclusive_only() {
    for players in 2..=4 {
        let mut game = Game::new(config(players, true));
        let choices = Space::extra_for(players);
        assert_eq!(choices.len(), if players == 2 { 4 } else { 2 });
        let action = if players == 2 {
            Action {
                space: Space::ResourceMarket,
                detail: Detail::None,
            }
        } else {
            SimpleDetails
                .produce(
                    &game.observation(game.public.active_player).unwrap(),
                    Space::AnimalMarket,
                )
                .unwrap()
        };
        game.apply(action).unwrap();
        assert!(choices.iter().all(|s| !game.legal_spaces().contains(s)));
        if players == 4 {
            assert!(game.legal_spaces().contains(&Space::ResourceMarket));
            assert!(game.legal_spaces().contains(&Space::Copse));
        }
        game.validate().unwrap();
    }
}

#[test]
fn market_choice_and_accumulation_supply() {
    let mut three = Game::new(config(3, false));
    let id = three.public.active_player;
    assert_eq!(three.public.accumulations[&Space::Grove], 2);
    assert_eq!(three.public.accumulations[&Space::Hollow], 1);
    assert!(three
        .apply(Action {
            space: Space::ResourceMarket,
            detail: Detail::None
        })
        .is_err());
    three
        .apply(Action {
            space: Space::ResourceMarket,
            detail: Detail::MarketChoice(Good::Reed),
        })
        .unwrap();
    assert_eq!(three.public.players[id].supply.reed, 1);
    assert_eq!(three.public.players[id].supply.stone, 0);
    assert_eq!(three.public.players[id].supply.food, 3);
    let mut four = Game::new(config(4, false));
    let id = four.public.active_player;
    assert_eq!(four.public.accumulations[&Space::Hollow], 2);
    assert_eq!(four.public.accumulations[&Space::TravelingPlayers], 1);
    four.apply(Action {
        space: Space::ResourceMarket,
        detail: Detail::None,
    })
    .unwrap();
    assert_eq!(four.public.players[id].supply.stone, 1);
    assert_eq!(four.public.players[id].supply.reed, 1);
}

#[test]
fn replay_and_save_restore_for_all_modes() {
    for players in 2..=4 {
        for extra in [false, true] {
            for seed in 0..2 {
                let cfg = Config {
                    seed,
                    additional_action_space_enabled: extra,
                    players,
                };
                let mut game = Game::new(cfg.clone());
                play_to_end(&mut game, &GreedyPolicy, &SimpleDetails).unwrap();
                assert!(game.public.finished);
                assert_eq!(game.public.round, 14);
                let replay = Game::replay(cfg, &game.log.events).unwrap();
                assert_eq!(game.state_hash(), replay.state_hash());
                let saved = game.save_json().unwrap();
                assert_eq!(
                    Game::load_json(&saved).unwrap().state_hash(),
                    game.state_hash()
                );
            }
        }
    }
}
