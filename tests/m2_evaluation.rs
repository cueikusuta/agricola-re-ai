use agricola_re_ai::benchmark::compare_policies;
use agricola_re_ai::engine::{Action, BakeAllocation, Detail};
use agricola_re_ai::evaluation::{
    action_value, action_value_with_details, evaluate_observation, evaluate_player,
    DetailAwareEvaluationPolicy, EvaluationPolicy, WeightedEvaluationPolicy,
};
use agricola_re_ai::model::{Config, Game, Good, Improvement, Observation, Player, Space, Tile};
use agricola_re_ai::policy::{
    play_to_end, DetailPolicy, GreedyPolicy, RandomPolicy, SimpleDetails, SpacePolicy,
};
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
    assert_eq!(early, 36.0);
    assert_eq!(late, 0.0);
}

struct BakeInsteadOfSow;

impl DetailPolicy for BakeInsteadOfSow {
    fn produce(&self, obs: &Observation, space: Space) -> Option<Action> {
        if space != Space::GrainUtilization {
            return SimpleDetails.produce(obs, space);
        }
        Some(Action {
            space,
            detail: Detail::SowDetailed {
                plantings: vec![],
                baking: vec![BakeAllocation {
                    improvement: Improvement::Hearth4,
                    grain: 1,
                }],
            },
        })
    }
}

struct DeclineGrain;

impl DetailPolicy for DeclineGrain {
    fn produce(&self, obs: &Observation, space: Space) -> Option<Action> {
        if space == Space::GrainUtilization {
            None
        } else {
            SimpleDetails.produce(obs, space)
        }
    }
}

#[test]
fn detail_aware_evaluation_uses_the_selected_action_plan() {
    let game = Game::new(Config::default());
    let mut obs = game.observation(game.public.active_player).unwrap();
    let p = &mut obs.public.players[obs.player_id];
    p.supply.grain = 1;
    p.improvements.push(Improvement::Hearth4);
    p.farm[1].tile = Tile::Field {
        crop: None,
        remaining: 0,
    };

    let sow = action_value(&obs, Space::GrainUtilization).unwrap();
    let bake = action_value_with_details(&obs, Space::GrainUtilization, &BakeInsteadOfSow).unwrap();
    assert_ne!(sow, bake);
    assert_eq!(
        sow,
        action_value_with_details(&obs, Space::GrainUtilization, &SimpleDetails).unwrap()
    );

    let rejecting = DetailAwareEvaluationPolicy {
        weights: WeightedEvaluationPolicy {
            prior_weight: 0.5,
            state_weight: 2.0,
        },
        details: &DeclineGrain,
    };
    let mut rng = ChaCha8Rng::seed_from_u64(1);
    assert_eq!(
        rejecting.select(
            &obs,
            &[Space::GrainUtilization, Space::DayLaborer],
            &mut rng
        ),
        Some(Space::DayLaborer)
    );
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

#[test]
fn fixed_seed_m2_strength_ladder_holds_for_both_tile_modes() {
    for enabled in [false, true] {
        let baseline = compare_policies(&GreedyPolicy, &RandomPolicy, 12, 7100, enabled).unwrap();
        let upgraded =
            compare_policies(&EvaluationPolicy, &GreedyPolicy, 12, 7100, enabled).unwrap();
        assert!(
            baseline.left_win_rate > 0.5,
            "greedy should outperform random"
        );
        assert!(
            upgraded.left_win_rate > 0.5,
            "evaluation should outperform greedy"
        );
    }
}

#[test]
fn heldout_strength_check_for_both_tile_modes() {
    // Not part of the calibration seeds (4200, 7100, 16800).
    for enabled in [false, true] {
        let result =
            compare_policies(&EvaluationPolicy, &GreedyPolicy, 20, 26000, enabled).unwrap();
        assert!(
            result.left_win_rate > 0.6,
            "held-out seed failed for additional_action_space_enabled={enabled}: {result:?}"
        );
    }
}
