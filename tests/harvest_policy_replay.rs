use agricola_re_ai::harvest::{FeedSource, FeedingPlan, HarvestDetailPolicy};
use agricola_re_ai::model::{Config, Game, Observation};
use agricola_re_ai::policy::{play_to_end_with_harvest_policy, RandomPolicy, SimpleDetails};

struct GrainPreservingHarvest;
impl HarvestDetailPolicy for GrainPreservingHarvest {
    fn plan_feeding(&self, _obs: &Observation) -> FeedingPlan {
        let mut plan = FeedingPlan::default();
        plan.priorities.swap(5, 8);
        plan.preserve_pairs_first = false;
        plan
    }
}

#[test]
fn custom_harvest_policy_choices_are_recorded_and_replayed_without_policy() {
    for enabled in [false, true] {
        for seed in 0..15 {
            let config = Config {
                seed,
                additional_action_space_enabled: enabled,
            };
            let mut game = Game::new(config.clone());
            play_to_end_with_harvest_policy(
                &mut game,
                &RandomPolicy,
                &SimpleDetails,
                &GrainPreservingHarvest,
            )
            .unwrap();
            let logs = game
                .log
                .events
                .iter()
                .filter(|event| {
                    event.payload.to_string().contains("harvest_plans")
                        && !event.payload.to_string().contains("harvest_plans\\\":null")
                })
                .count();
            assert_eq!(logs, 6, "one feeding plan set for each of six harvests");
            assert!(game.public.finished);
            let replayed = Game::replay(config, &game.log.events).unwrap();
            assert_eq!(replayed.state_hash(), game.state_hash());
        }
    }
}

#[test]
fn order_validation_rejects_duplicate_sources() {
    let mut plan = FeedingPlan::default();
    plan.priorities[0] = FeedSource::Cattle;
    assert!(plan.validate().is_err());
}
