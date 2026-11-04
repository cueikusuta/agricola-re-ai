use agricola_re_ai::model::{Config, Game, HARVEST_ROUNDS};
use agricola_re_ai::policy::{play_to_end, RandomPolicy, SimpleDetails};

#[test]
fn only_the_six_official_rounds_have_harvest() {
    assert_eq!(HARVEST_ROUNDS, [4, 7, 9, 11, 13, 14]);
}

#[test]
fn full_games_apply_harvest_without_violating_invariants() {
    for enabled in [false, true] {
        for seed in 0..25 {
            let mut g = Game::new(Config { players: 2,
                seed,
                additional_action_space_enabled: enabled,
            });
            play_to_end(&mut g, &RandomPolicy, &SimpleDetails).unwrap();
            g.validate().unwrap();
            assert!(g.public.finished);
            assert!(g.public.players.iter().all(|p| p.newborns <= p.family));
        }
    }
}
