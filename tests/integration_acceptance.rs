use agricola_re_ai::model::{Config, Game};
use agricola_re_ai::policy::{play_to_end, GreedyPolicy, RandomPolicy, SimpleDetails};
use agricola_re_ai::score::standings;

#[test]
fn one_hundred_seeded_games_per_mode_are_replayable() {
    for enabled in [false, true] {
        for seed in 0..100 {
            let config = Config {
                seed,
                additional_action_space_enabled: enabled,
            };
            let mut g = Game::new(config.clone());
            play_to_end(&mut g, &RandomPolicy, &SimpleDetails).unwrap();
            let replayed = Game::replay(config, &g.log.events).unwrap();
            assert_eq!(replayed.state_hash(), g.state_hash());
            let saved = g.save_json().unwrap();
            let restored = Game::load_json(&saved).unwrap();
            assert_eq!(restored.state_hash(), g.state_hash());
            assert_eq!(standings(&g.public.players).scores.len(), 2);
        }
    }
}

#[test]
fn greedy_complete_game_replays_in_both_modes() {
    for enabled in [false, true] {
        let config = Config {
            seed: 9183,
            additional_action_space_enabled: enabled,
        };
        let mut g = Game::new(config.clone());
        play_to_end(&mut g, &GreedyPolicy, &SimpleDetails).unwrap();
        assert!(g.public.finished);
        assert_eq!(Game::replay(config, &g.log.events).unwrap().state_hash(), g.state_hash());
    }
}
