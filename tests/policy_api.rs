use agricola_re_ai::engine::{Action, Detail};
use agricola_re_ai::model::{Config, Game, Observation, Space};
use agricola_re_ai::policy::{DetailPolicy, GreedyPolicy, RandomPolicy, SpacePolicy};
use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;

struct CustomDetails;
impl DetailPolicy for CustomDetails {
    fn produce(&self, _: &Observation, space: Space) -> Option<Action> {
        match space {
            Space::DayLaborer => Some(Action {
                space,
                detail: Detail::None,
            }),
            _ => None,
        }
    }
}

#[test]
fn decision_strategies_receive_only_public_observations() {
    let g = Game::new(Config {
        seed: 15,
        additional_action_space_enabled: false,
    });
    let obs = g.observation(g.public.active_player).unwrap();
    let json = serde_json::to_string(&obs).unwrap();
    assert!(!json.contains("hidden_round_deck"));
    assert!(!json.contains("action_seed"));
    assert!(!json.contains("round_seed"));
    let legal = g.legal_spaces();
    let mut rng = ChaCha8Rng::seed_from_u64(7);
    for strategy in [&RandomPolicy as &dyn SpacePolicy, &GreedyPolicy] {
        let selected = strategy.select(&obs, &legal, &mut rng).unwrap();
        assert!(legal.contains(&selected));
    }
    assert_eq!(
        CustomDetails.produce(&obs, Space::DayLaborer),
        Some(Action { space: Space::DayLaborer, detail: Detail::None })
    );
}
