use agricola_re_ai::cards::{CardDefinition, CardEffect, CardKind, CardSet};
use agricola_re_ai::engine::{Action, Detail};
use agricola_re_ai::model::{Config, Game, Good, Space};
use agricola_re_ai::score::score;

fn set(players: usize) -> CardSet {
    let mut definitions = vec![];
    for i in 0..7 * players {
        definitions.push(CardDefinition {
            id: format!("occ_{i}"), kind: CardKind::Occupation, cost: vec![], points: 0,
            effects: vec![CardEffect::AfterWorkerGain { space: Space::GrainSeeds,
                good: Good::Food, amount: 2 }],
        });
        definitions.push(CardDefinition {
            id: format!("minor_{i}"), kind: CardKind::MinorImprovement, cost: vec![],
            points: 2, effects: vec![CardEffect::OnPlayGain { good: Good::Wood, amount: 1 }],
        });
    }
    CardSet { definitions }
}
#[test]
fn occupation_and_minor_replay_rollback_and_scoring() {
    let config = Config { players: 2, seed: 33, ..Config::default() };
    let mut game = Game::with_cards(config.clone(), set(2)).unwrap();
    let start = game.public.active_player;
    let card_id = game.observation(start).unwrap().hand.iter().find(|id| id.starts_with("occ_")).unwrap().clone();
    let food = game.public.players[start].supply.food;
    game.apply(Action { space: Space::Lessons, detail: Detail::PlayOccupation { card_id } }).unwrap();
    assert_eq!(game.public.players[start].supply.food, food);
    assert_eq!(game.public.players[start].played_cards.len(), 1);
    assert_eq!(game.observation(start).unwrap().hand.len(), 13);
    let state = game.state_hash();
    assert!(game.apply(Action { space: Space::Lessons, detail: Detail::None }).is_err());
    assert_eq!(game.state_hash(), state);
    let next = game.public.active_player;
    let minor_id = game.observation(next).unwrap().hand.iter().find(|id| id.starts_with("minor_")).unwrap().clone();
    game.apply(Action { space: Space::MeetingPlace, detail: Detail::PlayMinor { card_id: minor_id } }).unwrap();
    assert_eq!(score(&game.public.players[next]).categories["cards"], 2);
    assert_eq!(game.public.players[next].supply.wood, 1);
    let events = game.log.events.clone();
    let replay = Game::replay_with_cards(config.clone(), set(2), &events).unwrap();
    assert_eq!(replay.state_hash(), game.state_hash());
    let restore = Game::load_json(&game.save_json().unwrap()).unwrap();
    assert_eq!(restore.state_hash(), game.state_hash());
}
#[test]
fn handless_game_cannot_use_lessons_or_sneak_minor() {
    let mut game = Game::new(Config::default());
    assert!(!game.legal_spaces().contains(&Space::Lessons));
    assert!(!game.legal_spaces().contains(&Space::ExtraLessons));
    let before = game.state_hash();
    assert!(game.apply(Action { space: Space::MeetingPlace,
        detail: Detail::PlayMinor { card_id: "minor_0".into() } }).is_err());
    assert_eq!(before, game.state_hash());
}
