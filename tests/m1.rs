use agricola_re_ai::engine::{Action, Detail, FreeAction};
use agricola_re_ai::model::{Config, Game, Good, RoundCard, Space, Tile};
use agricola_re_ai::policy::{play_to_end, GreedyPolicy, RandomPolicy, SimpleDetails};
use agricola_re_ai::score::standings;

fn game(seed: u64, enabled: bool) -> Game {
    Game::new(Config {
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
    assert_eq!(g.log.log_version, 1);
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
            seed: 222,
            additional_action_space_enabled: true,
        },
        &g.log.events,
    )
    .unwrap();
    assert_eq!(replay.state_hash(), g.state_hash());
}
