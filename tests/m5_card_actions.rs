use agricola_re_ai::cards::{CardDefinition, CardEffect, CardKind, CardSet};
use agricola_re_ai::engine::{Action, Detail};
use agricola_re_ai::model::{Config, Game, Good, Space};
use agricola_re_ai::score::score;

fn set(players: usize) -> CardSet {
    let mut definitions = vec![];
    for i in 0..7 * players {
        definitions.push(CardDefinition {
            use_limit: None,
            id: format!("occ_{i}"),
            kind: CardKind::Occupation,
            cost: vec![],
            points: 0,
            effects: vec![CardEffect::AfterWorkerGain {
                space: Space::GrainSeeds,
                good: Good::Food,
                amount: 2,
            }],
        });
        definitions.push(CardDefinition {
            use_limit: None,
            id: format!("minor_{i}"),
            kind: CardKind::MinorImprovement,
            cost: vec![],
            points: 2,
            effects: vec![CardEffect::OnPlayGain {
                good: Good::Wood,
                amount: 1,
            }],
        });
    }
    CardSet { definitions }
}
#[test]
fn occupation_and_minor_replay_rollback_and_scoring() {
    let config = Config {
        players: 2,
        seed: 33,
        ..Config::default()
    };
    let mut game = Game::with_cards(config.clone(), set(2)).unwrap();
    let start = game.public.active_player;
    let card_id = game
        .observation(start)
        .unwrap()
        .hand
        .iter()
        .find(|id| id.starts_with("occ_"))
        .unwrap()
        .clone();
    let food = game.public.players[start].supply.food;
    game.apply(Action {
        space: Space::Lessons,
        detail: Detail::PlayOccupation { card_id },
    })
    .unwrap();
    assert_eq!(game.public.players[start].supply.food, food);
    assert_eq!(game.public.players[start].played_cards.len(), 1);
    assert_eq!(game.observation(start).unwrap().hand.len(), 13);
    let state = game.state_hash();
    assert!(game
        .apply(Action {
            space: Space::Lessons,
            detail: Detail::None
        })
        .is_err());
    assert_eq!(game.state_hash(), state);
    let next = game.public.active_player;
    let minor_id = game
        .observation(next)
        .unwrap()
        .hand
        .iter()
        .find(|id| id.starts_with("minor_"))
        .unwrap()
        .clone();
    game.apply(Action {
        space: Space::MeetingPlace,
        detail: Detail::PlayMinor { card_id: minor_id },
    })
    .unwrap();
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
    assert!(game
        .apply(Action {
            space: Space::MeetingPlace,
            detail: Detail::PlayMinor {
                card_id: "minor_0".into()
            }
        })
        .is_err());
    assert_eq!(before, game.state_hash());
}

#[test]
fn c077_play_reserves_clay_and_the_ledger_survives_save_and_replay() {
    use agricola_re_ai::cards::{verified_band_c_cards, CardSet};
    use agricola_re_ai::model::DelayedGood;

    let mut definitions = verified_band_c_cards();
    let mut hands = vec![vec![], vec![]];
    for seat in 0..2 {
        for i in 0..7 {
            let occupation = format!("occ_{seat}_{i}");
            definitions.push(CardDefinition {
                use_limit: None,
                id: occupation.clone(),
                kind: CardKind::Occupation,
                cost: vec![],
                points: 0,
                effects: vec![],
            });
            hands[seat].push(occupation);
        }
        for i in 0..7 {
            let minor = format!("minor_{seat}_{i}");
            definitions.push(CardDefinition {
                use_limit: None,
                id: minor.clone(),
                kind: CardKind::MinorImprovement,
                cost: vec![],
                points: 0,
                effects: vec![],
            });
            hands[seat].push(minor);
        }
    }
    // Seat 0 starts with C077 instead of one filler minor.
    hands[0].retain(|id| id != "minor_0_0");
    hands[0].push("C077".into());
    let set = CardSet { definitions };
    let config = Config {
        players: 2,
        seed: 33,
        ..Config::default()
    };
    let mut game =
        Game::with_cards_and_hands_with_start_player(config.clone(), set.clone(), hands.clone(), 0)
            .unwrap();
    assert_eq!(game.public.active_player, 0);

    game.apply(Action {
        space: Space::MeetingPlace,
        detail: Detail::PlayMinor {
            card_id: "C077".into(),
        },
    })
    .unwrap();

    assert_eq!(game.public.players[0].played_cards, vec!["C077"]);
    let expected = vec![2u8, 3, 4];
    assert_eq!(
        game.delayed_goods.keys().copied().collect::<Vec<_>>(),
        expected
    );
    assert!(game.delayed_goods.values().all(|entries| entries
        == &vec![DelayedGood {
            player: 0,
            good: Good::Clay,
            amount: 1,
        }]));
    assert_eq!(
        game.observation(0).unwrap().delayed_goods_schedule,
        game.delayed_goods,
        "the reservation is public information"
    );

    // Replay and save/restore must reproduce the ledger, and the ledger must
    // stay outside the LOG_VERSION=5 state hash while being covered by the
    // bookkeeping integrity hash.
    let events = game.log.events.clone();
    let replay =
        Game::replay_with_setup_and_start_player(config, set, Some(hands), Some(0), &events)
            .unwrap();
    assert_eq!(replay.state_hash(), game.state_hash());
    assert_eq!(replay.delayed_goods, game.delayed_goods);
    let restored = Game::load_json(&game.save_json().unwrap()).unwrap();
    assert_eq!(restored.state_hash(), game.state_hash());
    assert_eq!(restored.delayed_goods, game.delayed_goods);

    let mut without_ledger = game.clone();
    without_ledger.delayed_goods.clear();
    assert_eq!(
        without_ledger.state_hash(),
        game.state_hash(),
        "the ledger is excluded from the v5 state hash"
    );
    assert_ne!(
        without_ledger.integrity_hash(),
        game.integrity_hash(),
        "the ledger is covered by the integrity hash"
    );
}
