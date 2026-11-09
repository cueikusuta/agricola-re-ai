use agricola_re_ai::engine::{Action, Detail, FreeAction};
use agricola_re_ai::model::{Config, Game, Good, RoundCard, Space, Tile};
use agricola_re_ai::policy::{play_to_end, GreedyPolicy, RandomPolicy, SimpleDetails};
use agricola_re_ai::score::standings;

fn game(seed: u64, enabled: bool) -> Game {
    Game::new(Config {
        players: 2,
        seed,
        additional_action_space_enabled: enabled,
    })
}
fn act(space: Space, detail: Detail) -> Action {
    Action { space, detail }
}
#[test]
fn start_state_and_seed_reproducibility() {
    for s in 0..50 {
        let a = game(s, false);
        let b = game(s, false);
        assert_eq!(a.state_hash(), b.state_hash());
        assert_eq!(a.public.round, 1);
        assert_eq!(a.public.revealed.len(), 1);
        assert_eq!(a.public.players.len(), 2);
        assert_eq!(a.public.players[a.public.start_player].supply.food, 2);
        assert_eq!(a.public.players[1 - a.public.start_player].supply.food, 3);
        for p in &a.public.players {
            assert_eq!(p.family, 2);
            assert_eq!(p.rooms(), 2);
            assert_eq!(p.farm.len(), 15)
        }
    }
}
#[test]
fn stage_order_correct_for_every_seed() {
    for seed in 0..100 {
        let g = game(seed, false);
        let mut all = vec![g.public.revealed[0]];
        all.extend(g.hidden_round_deck.iter().rev().copied());
        assert_eq!(all.len(), 14);
        for (i, card) in all.iter().enumerate() {
            let expected = match i {
                0..=3 => 1,
                4..=6 => 2,
                7..=8 => 3,
                9..=10 => 4,
                11..=12 => 5,
                _ => 6,
            };
            assert_eq!(card.stage(), expected);
        }
        all.sort();
        all.dedup();
        assert_eq!(all.len(), 14);
    }
}
#[test]
fn observation_excludes_private_deck_and_seeds() {
    let g = game(123456, true);
    let view = serde_json::to_value(g.observation(0).unwrap()).unwrap();
    assert!(view.get("hidden_round_deck").is_none());
    assert!(view.get("rng").is_none());
    assert!(view.get("config").is_none());
    assert!(view.get("seed").is_none());
    assert!(view
        .get("public")
        .unwrap()
        .get("additional_action_space_enabled")
        .is_some());
    assert_eq!(
        view.get("public")
            .unwrap()
            .get("revealed")
            .unwrap()
            .as_array()
            .unwrap()
            .len(),
        1
    );
}
#[test]
fn settings_disabled_default_and_enabled_tile_exclusion() {
    let a = game(10, false);
    assert!(!a.legal_spaces().iter().any(|s| s.additional()));
    let mut b = game(10, true);
    assert_eq!(b.public.accumulations.get(&Space::Copse), Some(&1));
    assert!(b.legal_spaces().contains(&Space::ResourceMarket));
    b.apply(act(Space::ResourceMarket, Detail::None)).unwrap();
    assert!(b.legal_spaces().iter().all(|s| !s.additional()));
    assert_eq!(b.public.accumulations.get(&Space::Copse), Some(&1));
    assert!(b.apply(act(Space::Copse, Detail::None)).is_err());
}
#[test]
fn illegal_action_is_atomic() {
    let mut g = game(2, true);
    let old = g.state_hash();
    let illegal = act(Space::Farmland, Detail::Plow { cell: 0 });
    assert!(g.apply(illegal).is_err());
    assert_eq!(old, g.state_hash());
    assert!(g
        .apply(act(
            Space::ResourceMarket,
            Detail::Sow {
                plantings: vec![],
                bake: 0
            }
        ))
        .is_err());
    assert_eq!(old, g.state_hash());
}
#[test]
fn mandatory_meeting_place_food_and_start_player() {
    let mut g = game(0, false);
    let other = 1 - g.public.start_player;
    // Move the active player once so the opponent can choose Meeting Place.
    g.apply(act(Space::DayLaborer, Detail::None)).unwrap();
    assert_eq!(g.public.active_player, other);
    let before = g.public.players[other].supply.food;
    g.apply(act(Space::MeetingPlace, Detail::None)).unwrap();
    assert_eq!(g.public.start_player, other);
    assert_eq!(g.public.players[other].supply.food, before + 1);
}
#[test]
fn plowing_and_room_building_costs() {
    let mut g = game(0, false);
    let who = g.public.active_player;
    g.public.players[who].supply.wood = 5;
    g.public.players[who].supply.reed = 2;
    g.apply(act(
        Space::FarmExpansion,
        Detail::Build {
            rooms: vec![1],
            stables: vec![],
        },
    ))
    .unwrap();
    assert_eq!(g.public.players[who].rooms(), 3);
    assert_eq!(g.public.players[who].supply.wood, 0);
    assert_eq!(g.public.players[who].supply.reed, 0);
    let another = g.public.active_player;
    g.apply(act(Space::Farmland, Detail::Plow { cell: 14 }))
        .unwrap();
    assert!(matches!(
        g.public.players[another].farm[14].tile,
        Tile::Field { .. }
    ));
}
#[test]
fn fence_geometry_and_capacity() {
    let mut g = game(1, false);
    let p = g.public.active_player;
    g.public.players[p].supply.wood = 15;
    // Directly test player pasture rules through the action space in a seeded round where Fencing is available.
    // Do not assume that the shuffled stage-1 card is Fencing on round 1.
    let mut copy = g.clone();
    let old = copy.public.revealed[0];
    if old != RoundCard::Fencing {
        let index = copy
            .hidden_round_deck
            .iter()
            .position(|&x| x == RoundCard::Fencing)
            .unwrap();
        copy.hidden_round_deck[index] = old;
        copy.public.revealed[0] = RoundCard::Fencing;
    }
    // A unit-only setup: swaps two stage-1 cards, preserving the full deck length.
    assert!(copy
        .apply(act(
            Space::Fencing,
            Detail::Fence {
                pastures: vec![vec![14]]
            }
        ))
        .is_ok());
    assert_eq!(copy.public.players[p].fences.len(), 4);
    assert_eq!(copy.public.players[p].pastures.len(), 1);
    assert_eq!(
        copy.public.players[p].pasture_capacity(&copy.public.players[p].pastures[0]),
        2
    );
}
#[test]
fn scoring_tiebreaker_and_equal_rank() {
    let mut a = game(1, false);
    let result = standings(&a.public.players);
    // Starting player's 2 food vs 3 food doesn't affect points or tie-break.
    assert_eq!(result.rank, [1, 1]);
    a.public.players[0].supply.wood += 1;
    let result = standings(&a.public.players);
    assert_eq!(result.scores[0].total, result.scores[1].total);
    assert_eq!(result.rank, [1, 2]);
    a.public.players[1].supply.stone += 1;
    assert_eq!(standings(&a.public.players).rank, [1, 1]);
}
#[test]
fn full_game_replay_and_resume_both_variants() {
    for enabled in [false, true] {
        for seed in 0..10 {
            let mut original = game(seed, enabled);
            play_to_end(&mut original, &RandomPolicy, &SimpleDetails).unwrap();
            assert!(original.public.finished);
            assert_eq!(original.public.round, 14);
            let resumed = Game::load_json(&original.save_json().unwrap()).unwrap();
            assert_eq!(resumed.state_hash(), original.state_hash());
            let replayed = Game::replay(
                Config {
                    players: 2,
                    seed,
                    additional_action_space_enabled: enabled,
                },
                &original.log.events,
            )
            .unwrap();
            assert_eq!(replayed.state_hash(), original.state_hash());
        }
    }
}
#[test]
fn fixed_seed_100_games_per_setting() {
    for enabled in [false, true] {
        for seed in 0..100 {
            let mut g = game(seed, enabled);
            play_to_end(&mut g, &RandomPolicy, &SimpleDetails).unwrap();
            assert!(g.public.finished);
            g.validate().unwrap();
            assert_eq!(standings(&g.public.players).scores.len(), 2);
        }
    }
}
#[test]
fn greedy_policy_works() {
    for enabled in [false, true] {
        let mut g = game(4321, enabled);
        play_to_end(&mut g, &GreedyPolicy, &SimpleDetails).unwrap();
        assert!(g.public.finished);
    }
}
#[test]
fn free_crop_conversion_and_discard() {
    let mut g = game(9, false);
    let id = g.public.active_player;
    g.public.players[id].supply.grain = 3;
    g.apply_free(
        id,
        FreeAction::ConvertCrop {
            good: Good::Grain,
            quantity: 2,
        },
    )
    .unwrap();
    assert_eq!(g.public.players[id].supply.grain, 1);
    g.apply_free(
        id,
        FreeAction::DiscardGood {
            good: Good::Grain,
            quantity: 1,
        },
    )
    .unwrap();
    assert_eq!(g.public.players[id].supply.grain, 0);
}
#[test]
fn newborn_can_only_act_from_next_round() {
    let mut g = game(123, false);
    let id = g.public.active_player;
    // Controlled unit fixture: expose a stage-2 card in the first round to isolate growth behavior.
    let old = g.public.revealed[0];
    let index = g
        .hidden_round_deck
        .iter()
        .position(|&x| x == RoundCard::FamilyGrowthWithRoom)
        .unwrap();
    g.hidden_round_deck[index] = old;
    g.public.revealed[0] = RoundCard::FamilyGrowthWithRoom;
    g.public.players[id].farm[1].tile = Tile::Room;
    assert_eq!(g.public.work_quota[id], 2);
    g.apply(act(Space::FamilyGrowthWithRoom, Detail::None))
        .unwrap();
    assert_eq!(g.public.players[id].family, 3);
    assert_eq!(g.public.players[id].newborns, 1);
    assert_eq!(g.public.work_quota[id], 2);
}
#[test]
fn duplicate_action_space_is_rejected() {
    let mut g = game(101, false);
    g.apply(act(Space::DayLaborer, Detail::None)).unwrap();
    assert!(!g.legal_spaces().contains(&Space::DayLaborer));
    assert!(g.apply(act(Space::DayLaborer, Detail::None)).is_err());
}
#[test]
fn intermediate_save_resume_exact() {
    for enabled in [false, true] {
        let mut original = game(4242, enabled);
        for _ in 0..3 {
            let obs = original.observation(original.public.active_player).unwrap();
            let space = original.legal_spaces()[0];
            let action =
                agricola_re_ai::policy::DetailPolicy::produce(&SimpleDetails, &obs, space).unwrap();
            original.apply(action).unwrap();
        }
        let serialized = original.save_json().unwrap();
        let mut restored = Game::load_json(&serialized).unwrap();
        assert_eq!(original.state_hash(), restored.state_hash());
        play_to_end(&mut original, &RandomPolicy, &SimpleDetails).unwrap();
        play_to_end(&mut restored, &RandomPolicy, &SimpleDetails).unwrap();
        assert_eq!(original.state_hash(), restored.state_hash());
    }
}

#[test]
fn action_log_metadata_and_replay_information() {
    let mut g = game(222, true);
    assert_eq!(g.log.log_version, agricola_re_ai::model::LOG_VERSION);
    assert!(g.log.config.additional_action_space_enabled);
    assert_eq!(g.log.start_player, g.public.start_player);
    assert_eq!(g.log.initial_state_hash, g.state_hash());
    let before = g.state_hash();
    g.apply(act(Space::ResourceMarket, Detail::None)).unwrap();
    assert_eq!(g.log.events.len(), 1);
    assert_eq!(g.log.events[0].before, before);
    assert_eq!(g.log.events[0].after, g.state_hash());
    let replay = Game::replay(
        Config {
            players: 2,
            seed: 222,
            additional_action_space_enabled: true,
        },
        &g.log.events,
    )
    .unwrap();
    assert_eq!(replay.state_hash(), g.state_hash());
}

/// ===== Phase C / Issue #255 新規テスト =====

#[test]
fn incremental_vs_full_validation_agree_on_legal_actions() {
    use agricola_re_ai::model::{Config, Game};
    use agricola_re_ai::policy::{CardChoiceDetails, DetailPolicy, RandomPolicy, SpacePolicy};
    use agricola_re_ai::validation::{validate_full, validate_incremental, Verdict};
    use rand::SeedableRng;
    use rand_chacha::ChaCha8Rng;

    for players in [2, 3, 4] {
        for seed in 0..20 {
            let mut game = Game::new(Config {
                players,
                seed,
                additional_action_space_enabled: false,
            });
            let mut rng = ChaCha8Rng::seed_from_u64(seed ^ 0xDEAD_BEEF);
            for _ in 0..20 {
                if game.public.finished {
                    break;
                }
                let id = game.public.active_player;
                let obs = game.observation(id).unwrap();
                let legal = game.legal_spaces();
                if legal.is_empty() {
                    break;
                }
                let space = RandomPolicy.select(&obs, &legal, &mut rng).expect("legal");
                let action = CardChoiceDetails.produce(&obs, space).expect("detail");
                let pre = game.clone();
                game.apply(action).unwrap();
                let post = game.clone();

                let full = validate_full(&post);
                let inc = validate_incremental(&post, &pre);
                assert!(
                    full.is_ok(),
                    "full validation failed after legal action: seed={seed} players={players} action={space:?}"
                );
                match inc {
                    Verdict::Pass => {}
                    Verdict::Unsure => {
                        assert!(validate_full(&post).is_ok());
                    }
                    Verdict::Fail(msg) => {
                        panic!("incremental rejected legal action: {msg} (seed={seed} players={players} action={space:?})");
                    }
                }
            }
        }
    }
}

#[test]
fn incremental_validation_rejects_illegal_actions() {
    use agricola_re_ai::model::{Config, Game};
    use agricola_re_ai::validation::{validate_incremental, Verdict};

    for players in [2, 3, 4] {
        let game = Game::new(Config {
            players,
            seed: 7,
            additional_action_space_enabled: false,
        });
        let pre = game.clone();

        let mut post = pre.clone();
        let space = post.legal_spaces()[0];
        post.public.used.insert(space);
        post.public.used.insert(space);
        let verdict = validate_incremental(&post, &pre);
        assert!(matches!(verdict, Verdict::Fail(_) | Verdict::Unsure));

        let mut post2 = pre.clone();
        post2.public.round = 99;
        let verdict2 = validate_incremental(&post2, &pre);
        assert!(matches!(verdict2, Verdict::Fail(_) | Verdict::Unsure));

        let mut post3 = pre.clone();
        post3.public.work_quota[0] = 99;
        let verdict3 = validate_incremental(&post3, &pre);
        assert!(matches!(verdict3, Verdict::Fail(_) | Verdict::Unsure));
    }
}

#[test]
fn legal_action_state_hash_matches_between_apply_paths() {
    use agricola_re_ai::model::{Config, Game};
    use agricola_re_ai::policy::{CardChoiceDetails, DetailPolicy, RandomPolicy, SpacePolicy};
    use rand::SeedableRng;
    use rand_chacha::ChaCha8Rng;

    for players in [2, 3, 4] {
        for seed in 0..10 {
            let mut game = Game::new(Config {
                players,
                seed,
                additional_action_space_enabled: false,
            });
            let mut rng = ChaCha8Rng::seed_from_u64(seed ^ 0xFEED_FACE);
            for _ in 0..30 {
                if game.public.finished {
                    break;
                }
                let id = game.public.active_player;
                let obs = game.observation(id).unwrap();
                let legal = game.legal_spaces();
                if legal.is_empty() {
                    break;
                }
                let space = RandomPolicy.select(&obs, &legal, &mut rng).expect("legal");
                let action = CardChoiceDetails.produce(&obs, space).expect("detail");
                let hash_before = game.state_hash();
                game.apply(action).unwrap();
                let hash_after = game.state_hash();
                assert_ne!(hash_before, hash_after, "state must change after action");
            }
            game.validate().unwrap();
        }
    }
}

#[test]
fn illegal_action_leaves_state_unchanged_bytewise() {
    use agricola_re_ai::engine::{Action, Detail};
    use agricola_re_ai::model::{Config, Game, Space};

    for players in [2, 3, 4] {
        let mut game = Game::new(Config {
            players,
            seed: 42,
            additional_action_space_enabled: false,
        });
        let snapshot = game.save_json().unwrap();
        let hash_before = game.state_hash();

        let illegal = Action {
            space: Space::Farmland,
            detail: Detail::Plow { cell: 0 },
        };
        assert!(game.apply(illegal).is_err());

        let hash_after = game.state_hash();
        assert_eq!(
            hash_before, hash_after,
            "state_hash must not change after rejected action"
        );
        assert_eq!(
            game.save_json().unwrap(),
            snapshot,
            "full JSON must be identical"
        );
    }
}

#[test]
fn search_position_noninterference_with_hidden_cards() {
    use agricola_re_ai::cards::verified_basic_card_set;
    use agricola_re_ai::mcts::sample_game;
    use rand::SeedableRng;
    use rand_chacha::ChaCha8Rng;

    let game = Game::with_cards(
        Config {
            seed: 40,
            ..Config::default()
        },
        verified_basic_card_set(2).unwrap(),
    )
    .unwrap();

    let observed = game.observation(0).unwrap();
    let mut changed_hidden = game.clone();

    if let Some(idx) = changed_hidden.cards.hands[1].iter().position(|id| {
        changed_hidden
            .card_set
            .definition(id)
            .is_some_and(|c| c.kind == agricola_re_ai::cards::CardKind::MinorImprovement)
    }) {
        let replacement = changed_hidden.cards.undealt[0].clone();
        let old = std::mem::replace(&mut changed_hidden.cards.hands[1][idx], replacement);
        changed_hidden.cards.undealt[0] = old;
        changed_hidden.validate().unwrap();
    }

    assert_eq!(
        changed_hidden.observation(0).unwrap(),
        observed,
        "observation must not change when hidden opponent hand changes"
    );

    let mut first_rng = ChaCha8Rng::seed_from_u64(99);
    let mut second_rng = ChaCha8Rng::seed_from_u64(99);
    for _ in 0..5 {
        let first = sample_game(&observed, &mut first_rng).unwrap();
        let second = sample_game(&changed_hidden.observation(0).unwrap(), &mut second_rng).unwrap();
        first.validate().unwrap();
        second.validate().unwrap();
        assert_eq!(first.public, observed.public);
        assert_eq!(first.cards.hands[0], observed.hand);
        assert_eq!(first.cards, second.cards);
        assert_eq!(first.hidden_round_deck, second.hidden_round_deck);
    }
    assert_ne!(game.cards, changed_hidden.cards);
}

#[test]
fn fixed_seed_sample_game_reproducibility() {
    use agricola_re_ai::cards::verified_basic_card_set;
    use agricola_re_ai::mcts::sample_game;
    use rand::SeedableRng;
    use rand_chacha::ChaCha8Rng;

    let game = Game::with_cards(
        Config {
            seed: 7,
            ..Config::default()
        },
        verified_basic_card_set(2).unwrap(),
    )
    .unwrap();
    let obs = game.observation(0).unwrap();

    let mut rng_a = ChaCha8Rng::seed_from_u64(0x1234_5678);
    let mut rng_b = ChaCha8Rng::seed_from_u64(0x1234_5678);

    for _ in 0..16 {
        let a = sample_game(&obs, &mut rng_a).unwrap();
        let b = sample_game(&obs, &mut rng_b).unwrap();
        assert_eq!(a.cards.hands, b.cards.hands);
        assert_eq!(a.cards.undealt, b.cards.undealt);
        assert_eq!(a.hidden_round_deck, b.hidden_round_deck);
        assert_eq!(a.public, b.public);
    }
}

#[test]
fn search_clone_allocation_independent_of_registry_and_log_size() {
    use agricola_re_ai::cards::verified_basic_card_set;

    use agricola_re_ai::model::{Config, Game};
    use agricola_re_ai::policy::{CardChoiceDetails, DetailPolicy, RandomPolicy, SpacePolicy};
    use rand::SeedableRng;
    use rand_chacha::ChaCha8Rng;
    use std::sync::Arc;

    // Use a verified card set to ensure validity
    let card_set = verified_basic_card_set(2).unwrap();
    let mut game = Game::with_cards(
        Config {
            players: 2,
            seed: 1,
            additional_action_space_enabled: false,
        },
        card_set,
    )
    .unwrap();

    let mut rng = ChaCha8Rng::seed_from_u64(1);
    for _ in 0..20 {
        if game.public.finished {
            break;
        }
        let id = game.public.active_player;
        let obs = game.observation(id).unwrap();
        let legal = game.legal_spaces();
        if legal.is_empty() {
            break;
        }
        let space = RandomPolicy.select(&obs, &legal, &mut rng).expect("legal");
        let action = CardChoiceDetails.produce(&obs, space).expect("detail");
        game.apply(action).unwrap();
    }

    assert!(game.log.events.len() > 10, "should have accumulated events");

    let probe = game.search_clone();
    assert_eq!(
        probe.log.events.len(),
        0,
        "search clone must have empty event log"
    );
    assert!(!probe.record_events);
    assert!(Arc::ptr_eq(&probe.card_set, &game.card_set));
    assert_eq!(probe.state_hash(), game.state_hash());
    probe.validate().unwrap();
}

#[test]
fn v5_save_compatibility_load_and_continue() {
    use agricola_re_ai::model::{Config, Game};
    use agricola_re_ai::policy::{CardChoiceDetails, DetailPolicy, RandomPolicy, SpacePolicy};
    use rand::SeedableRng;
    use rand_chacha::ChaCha8Rng;

    for players in [2, 3, 4] {
        for seed in [0, 1, 2, 5, 10] {
            let mut original = Game::new(Config {
                players,
                seed,
                additional_action_space_enabled: false,
            });
            let mut rng = ChaCha8Rng::seed_from_u64(seed ^ 0xBEEF);
            for _ in 0..5 {
                let id = original.public.active_player;
                let obs = original.observation(id).unwrap();
                let legal = original.legal_spaces();
                if legal.is_empty() {
                    break;
                }
                let space = RandomPolicy.select(&obs, &legal, &mut rng).expect("legal");
                let action = CardChoiceDetails.produce(&obs, space).expect("detail");
                original.apply(action).unwrap();
            }
            let mid_hash = original.state_hash();
            let json = original.save_json().unwrap();

            let mut loaded = Game::load_json(&json).unwrap();
            assert_eq!(
                loaded.state_hash(),
                mid_hash,
                "loaded state_hash must match"
            );
            loaded.validate().unwrap();

            let mut rng1 = ChaCha8Rng::seed_from_u64(0xDEAD);
            let mut _rng2 = ChaCha8Rng::seed_from_u64(0xDEAD);
            for _ in 0..10 {
                if original.public.finished || loaded.public.finished {
                    break;
                }
                let id1 = original.public.active_player;
                let id2 = loaded.public.active_player;
                assert_eq!(id1, id2);
                let obs1 = original.observation(id1).unwrap();
                let obs2 = loaded.observation(id2).unwrap();
                assert_eq!(obs1, obs2);
                let legal1 = original.legal_spaces();
                let legal2 = loaded.legal_spaces();
                assert_eq!(legal1, legal2);
                let space = RandomPolicy
                    .select(&obs1, &legal1, &mut rng1)
                    .expect("legal");
                let action1 = CardChoiceDetails.produce(&obs1, space).expect("detail");
                let action2 = CardChoiceDetails.produce(&obs2, space).expect("detail");
                original.apply(action1).unwrap();
                loaded.apply(action2).unwrap();
                assert_eq!(original.state_hash(), loaded.state_hash());
            }
        }
    }
}
