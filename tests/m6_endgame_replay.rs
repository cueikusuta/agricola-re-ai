use agricola_re_ai::cards::{CardDefinition, CardEffect, CardKind, CardSet, ScoreMetric};
use agricola_re_ai::engine::{Action, Detail};
use agricola_re_ai::evaluation::EvaluationPolicy;
use agricola_re_ai::model::{Config, Game, Good, Space};
use agricola_re_ai::policy::{DetailPolicy, SimpleDetails, SpacePolicy};
use agricola_re_ai::score::score;
use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;

/// A complete deterministic test deck made from implemented effect primitives.
/// No proprietary text or unimplemented official cards are included.
fn fixture_set(players: usize) -> CardSet {
    let mut definitions = Vec::new();
    for n in 0..7 * players {
        definitions.push(CardDefinition {
            id: format!("occ-m6-{n}"),
            kind: CardKind::Occupation,
            cost: vec![],
            points: 0,
            effects: vec![CardEffect::EndScoreThresholds {
                metric: ScoreMetric::Improvements,
                thresholds: vec![(1, 1)],
            }],
        });
        definitions.push(CardDefinition {
            id: format!("minor-m6-{n}"),
            kind: CardKind::MinorImprovement,
            cost: vec![],
            points: 0,
            effects: vec![CardEffect::OnPlayGain {
                good: Good::Food,
                amount: 1,
            }],
        });
    }
    CardSet { definitions }
}

#[test]
fn end_game_card_bonus_survives_all_14_rounds_save_and_event_replay() {
    let config = Config {
        players: 2,
        seed: 6126,
        ..Config::default()
    };
    let set = fixture_set(2);
    let mut game = Game::with_cards(config.clone(), set.clone()).unwrap();
    let first = game.public.active_player;
    let hand = game.observation(first).unwrap().hand;
    let minor_id = hand.iter().find(|id| id.starts_with("minor-m6")).unwrap().clone();
    let occupation_id = hand.iter().find(|id| id.starts_with("occ-m6")).unwrap().clone();

    game.apply(Action {
        space: Space::MeetingPlace,
        detail: Detail::PlayMinor { card_id: minor_id },
    })
    .unwrap();
    // Hand over one worker so the original player can take Lessons.
    game.apply(Action {
        space: Space::DayLaborer,
        detail: Detail::None,
    })
    .unwrap();
    assert_eq!(game.public.active_player, first);
    game.apply(Action {
        space: Space::Lessons,
        detail: Detail::PlayOccupation {
            card_id: occupation_id,
        },
    })
    .unwrap();

    for turn in 0..250 {
        if game.public.finished {
            break;
        }
        let id = game.public.active_player;
        let obs = game.observation(id).unwrap();
        let legal = game.legal_spaces();
        let mut rng = ChaCha8Rng::seed_from_u64(turn + 9871);
        let space = EvaluationPolicy.select(&obs, &legal, &mut rng).unwrap();
        let action = SimpleDetails.produce(&obs, space).unwrap();
        game.apply(action).unwrap();
    }

    assert!(game.public.finished, "match must reach round 14");
    assert_eq!(game.public.round, 14);
    assert_eq!(game.public.players[first].card_bonus_points, 1);
    assert_eq!(score(&game.public.players[first]).categories["cards"], 1);
    game.validate().unwrap();

    let saved = game.save_json().unwrap();
    let loaded = Game::load_json(&saved).unwrap();
    assert_eq!(loaded.state_hash(), game.state_hash());
    let replay = Game::replay_with_cards(config, set, &game.log.events).unwrap();
    assert_eq!(replay.state_hash(), game.state_hash());
    assert_eq!(replay.public.players[first].card_bonus_points, 1);
}

#[test]
fn tampered_condition_bonus_is_rejected_on_load() {
    let config = Config::default();
    let mut game = Game::with_cards(config, fixture_set(2)).unwrap();
    game.public.players[0].card_bonus_points = 11;
    assert!(Game::load_json(&game.save_json().unwrap()).is_err());
}
