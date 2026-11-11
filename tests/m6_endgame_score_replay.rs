//! Regression carried forward from the earlier endgame-score implementation.
use agricola_re_ai::cards::{CardDefinition, CardEffect, CardKind, CardSet, ScoreBonusRule};
use agricola_re_ai::engine::{Action, Detail};
use agricola_re_ai::evaluation::EvaluationPolicy;
use agricola_re_ai::model::{Config, Game, Space};
use agricola_re_ai::policy::{play_to_end, SimpleDetails};
use agricola_re_ai::score::{score, score_with_cards};

fn test_cards() -> CardSet {
    let mut definitions = Vec::new();
    for number in 0..14 {
        definitions.push(CardDefinition {
            use_limit: None,
            id: format!("occupation-{number}"),
            kind: CardKind::Occupation,
            cost: vec![],
            points: 0,
            effects: vec![],
        });
        definitions.push(CardDefinition {
            use_limit: None,
            id: format!("minor-{number}"),
            kind: CardKind::MinorImprovement,
            cost: vec![],
            points: 0,
            effects: vec![CardEffect::EndGameBonus {
                rule: ScoreBonusRule::HouseMaterial,
            }],
        });
    }
    CardSet { definitions }
}

#[test]
fn played_conditional_score_matches_full_match_and_replay() {
    let config = Config {
        seed: 839,
        players: 2,
        additional_action_space_enabled: false,
    };
    let set = test_cards();
    let mut game = Game::with_cards(config.clone(), set.clone()).unwrap();
    let first = game.public.active_player;
    let chosen = game.cards.hands[first]
        .iter()
        .find(|id| id.starts_with("minor-"))
        .unwrap()
        .clone();
    game.apply(Action {
        space: Space::MeetingPlace,
        detail: Detail::PlayMinor {
            card_id: chosen.clone(),
        },
    })
    .unwrap();
    assert!(game.public.players[first].played_cards.contains(&chosen));
    play_to_end(&mut game, &EvaluationPolicy, &SimpleDetails).unwrap();
    assert!(game.public.finished);
    assert_eq!(game.public.round, 14);
    game.validate().unwrap();

    let player = &game.public.players[first];
    let actual = score_with_cards(player, &set);
    let printed_only = score(player);
    let bonus = agricola_re_ai::cards::end_game_bonus(&set, player);
    assert_eq!(actual.categories["card_bonus"], bonus);
    assert_eq!(actual.total - printed_only.total, bonus);

    let saved = game.save_json().unwrap();
    let restored = Game::load_json(&saved).unwrap();
    assert_eq!(restored.state_hash(), game.state_hash());
    let replay = Game::replay_with_cards(config, set, &game.log.events).unwrap();
    assert_eq!(replay.state_hash(), game.state_hash());
    assert_eq!(
        score_with_cards(&replay.public.players[first], &replay.card_set).total,
        actual.total
    );
}
