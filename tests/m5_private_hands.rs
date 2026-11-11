use agricola_re_ai::cards::{CardDefinition, CardEffect, CardKind, CardSet};
use agricola_re_ai::model::{Config, Game, Good, Space};

fn sample_set(players: usize) -> CardSet {
    let mut definitions = Vec::new();
    for index in 0..7 * players {
        definitions.push(CardDefinition {
            use_limit: None,
            id: format!("occ_{index}"),
            kind: CardKind::Occupation,
            cost: vec![],
            points: 0,
            effects: vec![CardEffect::AfterWorkerGain {
                space: Space::GrainSeeds,
                good: Good::Food,
                amount: 1,
            }],
        });
        definitions.push(CardDefinition {
            use_limit: None,
            id: format!("minor_{index}"),
            kind: CardKind::MinorImprovement,
            cost: vec![],
            points: 1,
            effects: vec![CardEffect::OnPlayGain {
                good: Good::Wood,
                amount: 1,
            }],
        });
    }
    CardSet { definitions }
}
#[test]
fn private_deal_is_deterministic_and_observer_scoped() {
    for players in 2..=4 {
        let config = Config {
            seed: 910,
            players,
            ..Config::default()
        };
        let game = Game::with_cards(config.clone(), sample_set(players)).unwrap();
        let other = Game::with_cards(config, sample_set(players)).unwrap();
        assert_eq!(game.cards, other.cards);
        game.validate().unwrap();
        for id in 0..players {
            let obs = game.observation(id).unwrap();
            assert_eq!(obs.hand.len(), 14);
            assert_eq!(obs.hand_counts, vec![14; players]);
            assert_eq!(obs.hand, game.cards.hands[id]);
            for opponent in 0..players {
                if opponent != id {
                    assert_ne!(obs.hand, game.cards.hands[opponent]);
                }
            }
            let serialized = serde_json::to_value(&obs).unwrap();
            assert!(serialized.get("hands").is_none());
            assert!(serialized.get("undealt").is_none());
        }
        let saved = game.save_json().unwrap();
        let restored = Game::load_json(&saved).unwrap();
        assert_eq!(restored.state_hash(), game.state_hash());
    }
}
#[test]
fn card_set_is_all_or_nothing_and_tampering_rejected() {
    let missing = CardSet {
        definitions: sample_set(2)
            .definitions
            .into_iter()
            .filter(|c| c.id != "occ_0")
            .collect(),
    };
    assert!(Game::with_cards(Config::default(), missing).is_err());
    let mut game = Game::with_cards(Config::default(), sample_set(2)).unwrap();
    game.cards.hands.swap(0, 1);
    assert!(Game::load_json(&game.save_json().unwrap()).is_err());
    assert!(Game::new(Config::default()).validate().is_ok());
}
