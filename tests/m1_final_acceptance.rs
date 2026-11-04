use agricola_re_ai::model::{Config, Game};
use agricola_re_ai::policy::{play_to_end, GreedyPolicy, RandomPolicy, SimpleDetails, SpacePolicy};

fn assert_complete(config: Config, policy: &dyn SpacePolicy) {
    let mut original = Game::new(config.clone());
    let opening_observation = original.observation(original.public.active_player).unwrap();
    let encoded = serde_json::to_value(&opening_observation).unwrap();
    let public_json = serde_json::to_string(&encoded).unwrap();
    for forbidden in ["hidden_round_deck", "rng", "round_seed", "action_seed"] {
        assert!(
            !public_json.contains(forbidden),
            "AI observation leaked {forbidden}"
        );
    }
    let original_opening = original.state_hash();
    let saved_opening = original.save_json().unwrap();
    assert_eq!(
        Game::load_json(&saved_opening).unwrap().state_hash(),
        original_opening
    );
    play_to_end(&mut original, policy, &SimpleDetails).unwrap();
    assert!(original.public.finished);
    assert_eq!(original.public.round, 14);
    assert!(
        !original.log.events.is_empty(),
        "completed games must log placements"
    );
    let replay = Game::replay(config, &original.log.events).unwrap();
    assert_eq!(replay.state_hash(), original.state_hash());
    let loaded = Game::load_json(&original.save_json().unwrap()).unwrap();
    assert_eq!(loaded.state_hash(), original.state_hash());
    assert!(original.validate().is_ok());
}

#[test]
fn both_policies_are_replayable_for_100_seeds_in_both_modes() {
    for additional_action_space_enabled in [false, true] {
        for seed in 1000..1100 {
            let config = Config {
                seed,
                additional_action_space_enabled,
            };
            assert_complete(config.clone(), &RandomPolicy);
            assert_complete(config, &GreedyPolicy);
        }
    }
}
