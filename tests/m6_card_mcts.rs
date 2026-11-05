use agricola_re_ai::cards::{CardDefinition, CardEffect, CardKind, CardSet};
use agricola_re_ai::engine::{Action, Detail};
use agricola_re_ai::mcts::MctsPolicy;
use agricola_re_ai::model::{Config, Game, Good, Space};
use agricola_re_ai::policy::SpacePolicy;
use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;

fn cards() -> CardSet {
    let mut definitions = vec![];
    for i in 0..25 {
        definitions.push(CardDefinition {
            id: format!("occ_{i}"), kind: CardKind::Occupation, cost: vec![],
            points: 0, effects: vec![],
        });
        definitions.push(CardDefinition {
            id: format!("minor_{i}"), kind: CardKind::MinorImprovement,
            cost: vec![], points: 1,
            effects: vec![CardEffect::OnPlayGain { good: Good::Wood, amount: 1 }],
        });
    }
    CardSet { definitions }
}

#[test]
fn card_mode_mcts_chooses_legal_moves_after_known_play() {
    let mut game = Game::with_cards(Config::default(), cards()).unwrap();
    let id = game.public.active_player;
    let card = game.observation(id).unwrap().hand.iter()
        .find(|id| id.starts_with("occ_")).unwrap().clone();
    game.apply(Action { space: Space::Lessons,
        detail: Detail::PlayOccupation { card_id: card } }).unwrap();
    let obs = game.observation(game.public.active_player).unwrap();
    let legal = game.legal_spaces();
    let policy = MctsPolicy { simulations: 6, tree_depth: 2 };
    let mut a = ChaCha8Rng::seed_from_u64(72);
    let mut b = ChaCha8Rng::seed_from_u64(72);
    let first = policy.select(&obs, &legal, &mut a);
    assert_eq!(first, policy.select(&obs, &legal, &mut b));
    assert!(legal.contains(&first.unwrap()));
}
