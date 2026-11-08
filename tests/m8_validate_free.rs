//! M8-2 (#109/#206): free actions share one rule-engine validator with
//! a side-effect-free dry run. Neither successful nor rejected probes may
//! mutate the authoritative game, event log, or observed public state.
use agricola_re_ai::engine::FreeAction;
use agricola_re_ai::model::{Config, Game, Good};

fn game(players: usize, extra: bool) -> Game {
    Game::new(Config {
        players,
        seed: 42001,
        additional_action_space_enabled: extra,
    })
}

#[test]
fn validate_free_accepts_legal_discard_without_mutating_game() {
    for players in 2..=4 {
        for extra in [false, true] {
            let mut game = game(players, extra);
            let who = game.public.active_player;
            game.public.players[who].supply.wood = 3;
            let free = FreeAction::DiscardGood {
                good: Good::Wood,
                quantity: 1,
            };
            let before_hash = game.state_hash();
            let before_public = game.public.clone();
            let before_log = game.log.clone();
            game.validate_free(who, &free).unwrap();
            game.validate_free(who, &free).unwrap();
            assert_eq!(game.state_hash(), before_hash);
            assert_eq!(game.public, before_public);
            assert_eq!(game.log, before_log);

            let old_event_count = game.log.events.len();
            game.apply_free(who, free).unwrap();
            assert_eq!(game.public.players[who].supply.wood, 2);
            assert_eq!(game.log.events.len(), old_event_count + 1);
        }
    }
}

#[test]
fn validate_free_rejects_invalid_action_and_preserves_original() {
    let mut game = game(2, false);
    let who = game.public.active_player;
    game.public.players[who].supply.wood = 2;
    for action in [
        FreeAction::DiscardGood {
            good: Good::Wood,
            quantity: 0,
        },
        FreeAction::DiscardGood {
            good: Good::Wood,
            quantity: 3,
        },
        FreeAction::ConvertCrop {
            good: Good::Wood,
            quantity: 1,
        },
    ] {
        let before_hash = game.state_hash();
        let before_log = game.log.clone();
        assert!(game.validate_free(who, &action).is_err());
        assert_eq!(game.state_hash(), before_hash);
        assert_eq!(game.log, before_log);

        assert!(game.apply_free(who, action).is_err());
        assert_eq!(game.state_hash(), before_hash);
        assert_eq!(game.log, before_log);
    }
    let before_hash = game.state_hash();
    assert!(game
        .validate_free(
            game.public.players.len(),
            &FreeAction::DiscardGood {
                good: Good::Wood,
                quantity: 1,
            },
        )
        .is_err());
    assert_eq!(game.state_hash(), before_hash);
}

#[test]
fn validate_free_matches_crop_conversion_and_finished_rejection() {
    let mut game = game(3, true);
    let who = game.public.active_player;
    game.public.players[who].supply.grain = 2;
    let free = FreeAction::ConvertCrop {
        good: Good::Grain,
        quantity: 1,
    };
    let before = game.state_hash();
    let food = game.public.players[who].supply.food;
    assert!(game.validate_free(who, &free).is_ok());
    assert_eq!(game.state_hash(), before);
    game.apply_free(who, free.clone()).unwrap();
    assert_eq!(game.public.players[who].supply.grain, 1);
    assert_eq!(game.public.players[who].supply.food, food + 1);

    game.public.finished = true;
    let before_finished = game.state_hash();
    let before_log = game.log.clone();
    assert!(game.validate_free(who, &free).is_err());
    assert_eq!(game.state_hash(), before_finished);
    assert_eq!(game.log, before_log);
}
