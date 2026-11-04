use agricola_re_ai::breeding::{BirthChoice, BirthDestination, BreedingPlan};
use agricola_re_ai::engine::RecordedAction;
use agricola_re_ai::harvest::{FeedingPlan, HarvestDetailPolicy};
use agricola_re_ai::model::{Animal, Config, Game, Observation};
use agricola_re_ai::policy::{play_to_end_with_harvest_policy, RandomPolicy, SimpleDetails};

struct ReturnOffspring;
impl HarvestDetailPolicy for ReturnOffspring {
    fn plan_feeding(&self, _: &Observation) -> FeedingPlan {
        FeedingPlan::default()
    }

    fn plan_breeding(&self, observation: &Observation) -> BreedingPlan {
        let player = &observation.public.players[observation.player_id];
        BreedingPlan {
            births: [Animal::Sheep, Animal::Boar, Animal::Cattle]
                .into_iter()
                .filter(|species| player.animal_count(*species) >= 2)
                .map(|species| BirthChoice {
                    species,
                    destination: BirthDestination::Discard,
                })
                .collect(),
            before_birth: None,
        }
    }
}

#[test]
fn external_breeding_policy_is_logged_and_replayed_for_both_modes() {
    for additional_action_space_enabled in [false, true] {
        for seed in 0..20 {
            let config = Config { players: 2,
                seed,
                additional_action_space_enabled,
            };
            let mut game = Game::new(config.clone());
            play_to_end_with_harvest_policy(
                &mut game,
                &RandomPolicy,
                &SimpleDetails,
                &ReturnOffspring,
            )
            .unwrap();
            assert!(game.public.finished);
            let harvest_count = game
                .log
                .events
                .iter()
                .filter(|event| {
                    matches!(
                        serde_json::from_value::<RecordedAction>(event.payload.clone()),
                        Ok(RecordedAction::Work {
                            harvest_plans: Some(_),
                            breeding_plans: Some(_),
                            ..
                        })
                    )
                })
                .count();
            assert_eq!(harvest_count, 6);
            let replayed = Game::replay(config, &game.log.events).unwrap();
            assert_eq!(replayed.state_hash(), game.state_hash());
        }
    }
}
