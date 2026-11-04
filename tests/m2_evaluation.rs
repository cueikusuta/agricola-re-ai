use agricola_re_ai::benchmark::compare_policies;
use agricola_re_ai::evaluation::{
    action_value, evaluate_observation, evaluate_player, EvaluationPolicy,
};
use agricola_re_ai::model::{Config, Game, Good, Player};
use agricola_re_ai::policy::{play_to_end, GreedyPolicy, RandomPolicy, SimpleDetails, SpacePolicy};
use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;

#[test]
fn food_safety_rewards_covering_a_shortage() {
    let mut p = Player::new(0, 0);
    let starving = evaluate_player(&p, 14);
    p.supply.gain(Good::Food, 4);
    let fed = evaluate_player(&p, 14);
    assert!(fed.food_safety > starving.food_safety);
    assert!(fed.total() > starving.total());
}

#[test]
fn growth_is_more_valuable_earlier() {
    let p = Player::new(0, 4);
    let mut grown = p.clone();
    grown.family += 1;
    let early = evaluate_player(&grown, 2).family_growth - evaluate_player(&p, 2).family_growth;
    let late = evaluate_player(&grown, 14).family_growth - evaluate_player(&p, 14).family_growth;
    assert!(early > late);
}

#[test]
fn evaluation_and_actions_depend_only_on_public_observation() {
    let mut game = Game::new(Config {
        seed: 912,
        additional_action_space_enabled: true,
    });
    let original = game.observation(game.public.active_player).unwrap();
    let spaces = game.legal_spaces();
    game.hidden_round_deck.reverse();
    game.config.seed = 1;
    let modified = game.observation(game.public.active_player).unwrap();
    assert_eq!(
        evaluate_observation(&original, original.player_id),
        evaluate_observation(&modified, modified.player_id)
    );
    for space in &spaces {
        assert_eq!(
            action_value(&original, *space),
            action_value(&modified, *space)
        );
    }
    let mut rng1 = ChaCha8Rng::seed_from_u64(111);
    let mut rng2 = ChaCha8Rng::seed_from_u64(111);
    assert_eq!(
        EvaluationPolicy.select(&original, &spaces, &mut rng1),
        EvaluationPolicy.select(&modified, &spaces, &mut rng2)
    );
}

#[test]
fn evaluation_ai_finishes_and_replays_both_tile_settings() {
    for enabled in [false, true] {
        for seed in 40..45 {
            let mut game = Game::new(Config {
                seed,
                additional_action_space_enabled: enabled,
            });
            play_to_end(&mut game, &EvaluationPolicy, &SimpleDetails).unwrap();
            assert!(game.public.finished);
            let replay = Game::replay(game.config.clone(), &game.log.events).unwrap();
            assert_eq!(game.state_hash(), replay.state_hash());
        }
    }
}

#[test]
fn seat_swap_preserves_equal_policy_outcomes_and_determinism() {
    for enabled in [false, true] {
        let first = compare_policies(&RandomPolicy, &RandomPolicy, 3, 982, enabled).unwrap();
        let second = compare_policies(&RandomPolicy, &RandomPolicy, 3, 982, enabled).unwrap();
        assert_eq!(first, second);
        assert_eq!(first.games, 6);
        assert_eq!(first.left_wins, first.right_wins);
        assert_eq!(first.left_wins + first.right_wins + first.ties, 6);
        assert!((first.left_mean_score - first.right_mean_score).abs() < 1e-9);
        assert!((first.left_win_rate - 0.5).abs() < 1e-9);
    }
}

#[test]
fn evaluation_policy_plays_balanced_head_to_head_matches() {
    for enabled in [false, true] {
        let result = compare_policies(&EvaluationPolicy, &GreedyPolicy, 2, 20, enabled).unwrap();
        assert_eq!(result.games, 4);
        assert_eq!(result.left_wins + result.right_wins + result.ties, 4);
        assert!((0.0..=1.0).contains(&result.left_win_rate));
    }
}
