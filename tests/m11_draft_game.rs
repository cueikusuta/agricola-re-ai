//! M11 (#104): a completed draft must seed the real game without re-dealing.
//!
//! Every id here is either a synthetic test fixture (`FIX-...`) or an id from
//! the engine's implemented 2-player set. The fixture path is deliberately
//! separated from the production path: an unverified fixture card must never be
//! accepted by the production card set.
use agricola_re_ai::cards::{
    verified_basic_card_set, CardDefinition, CardKind as DefKind, CardSet,
};
use agricola_re_ai::draft::{
    cpu_choice, cpu_pick, Direction, DraftMode, DraftPool, DraftRuleset, DraftSetup, DraftState,
};
use agricola_re_ai::draft_game::{
    hands_from_completed_draft, pool_from_card_set, start_game_from_draft,
};
use agricola_re_ai::model::{Config, Game};
use agricola_re_ai::policy::{DetailPolicy, SimpleDetails};

fn play_to_end(state: &mut DraftState) {
    match state.mode {
        DraftMode::Deal7 => {}
        DraftMode::Choose10_7 => {
            for seat in 0..state.player_count {
                let token = state.current_token();
                let obs = state.observe(seat, token).unwrap();
                let (occ, minor) = cpu_choice(&obs).unwrap();
                state.commit_choice(seat, token, occ, minor).unwrap();
            }
        }
        _ => {
            let mut guard = 0;
            while !state.is_complete() {
                guard += 1;
                assert!(guard < 128, "draft did not terminate");
                let token = state.current_token();
                for seat in 0..state.player_count {
                    let obs = state.observe(seat, token).unwrap();
                    let commit = cpu_pick(&obs).unwrap();
                    state
                        .commit_pick(seat, token, commit.occupation, commit.minor)
                        .unwrap();
                }
            }
        }
    }
}

fn fixture_card_set(pool: &DraftPool) -> CardSet {
    let definition = |id: &String, kind: DefKind| CardDefinition {
        use_limit: None,
        id: id.clone(),
        kind,
        cost: vec![],
        points: 0,
        effects: vec![],
    };
    let mut definitions = Vec::new();
    for id in &pool.occupations {
        definitions.push(definition(id, DefKind::Occupation));
    }
    for id in &pool.minors {
        definitions.push(definition(id, DefKind::MinorImprovement));
    }
    CardSet { definitions }
}

fn fixture_setup(mode: DraftMode, players: usize, seed: u64) -> (DraftSetup, CardSet) {
    let n = mode.dealt_per_kind() * players;
    let pool = DraftPool::new(
        (0..n).map(|i| format!("FIX-OCC-{i:02}")).collect(),
        (0..n).map(|i| format!("FIX-MIN-{i:02}")).collect(),
    );
    let set = fixture_card_set(&pool);
    (
        DraftSetup {
            mode,
            ruleset: DraftRuleset::explicit(
                "test-explicit",
                Direction::Right,
                Direction::Right,
                true,
            ),
            pool,
            player_count: players,
            seed,
            deck_version: "fixture-v1".into(),
        },
        set,
    )
}

fn verified_setup(mode: DraftMode, seed: u64) -> (DraftSetup, CardSet) {
    let set = verified_basic_card_set(2).unwrap();
    let pool = pool_from_card_set(&set);
    (
        DraftSetup {
            mode,
            ruleset: DraftRuleset::explicit(
                "verified-basic-explicit",
                Direction::Right,
                Direction::Right,
                true,
            ),
            pool,
            player_count: 2,
            seed,
            deck_version: "verified-basic-2p-v1".into(),
        },
        set,
    )
}

fn game_config(players: usize, seed: u64) -> Config {
    Config {
        players,
        seed,
        additional_action_space_enabled: false,
    }
}

/// Advance the game with the engine's default detail policy so the event log
/// contains a non-trivial chain to save, load and replay.
fn advance(game: &mut Game, steps: usize) {
    for _ in 0..steps {
        if game.public.finished {
            return;
        }
        let active = game.public.active_player;
        let obs = game.observation(active).unwrap();
        let legal = game.legal_spaces();
        let action = legal
            .iter()
            .find_map(|&space| SimpleDetails.produce(&obs, space))
            .expect("a legal action with default details");
        game.apply(action).unwrap();
    }
}

fn assert_hand_matches_set(game: &Game, seat: usize, expected: &[String]) {
    let mut actual = game.cards.hands[seat].clone();
    let mut expected = expected.to_vec();
    actual.sort();
    expected.sort();
    assert_eq!(actual, expected, "seat {seat} hand mismatch");
}

#[test]
fn verified_draft77_hand_is_injected_without_random_redistribution() {
    let (setup, set) = verified_setup(DraftMode::Draft77, 2468);
    let mut state = setup.start().unwrap();
    play_to_end(&mut state);
    assert!(state.is_complete());

    let hands = hands_from_completed_draft(&state, &set).unwrap();
    assert_eq!(hands.len(), 2);
    for hand in &hands {
        assert_eq!(hand.len(), 14, "each seat keeps 7 + 7 cards");
    }
    // Every unselected dealt card is absent from every final hand.
    let selected: std::collections::BTreeSet<&String> = hands.iter().flatten().collect();
    for id in state.kind_of.keys() {
        if !selected.contains(id) {
            assert!(
                !hands.iter().flatten().any(|hand_id| hand_id == id),
                "unselected card {id} leaked into a hand"
            );
        }
    }

    let draft_hash = state.state_hash();
    let game = start_game_from_draft(&state, set.clone(), game_config(2, 7)).unwrap();
    assert_eq!(
        state.state_hash(),
        draft_hash,
        "starting a game must not mutate the draft"
    );
    assert_hand_matches_set(&game, 0, &hands[0]);
    assert_hand_matches_set(&game, 1, &hands[1]);
    assert_eq!(game.public.start_player, state.start_player);
    assert_eq!(game.log.start_player, state.start_player);
    assert_eq!(
        game.log.initial_start_player_override,
        Some(state.start_player)
    );
    assert_eq!(game.public.players[state.start_player].supply.food, 2);
    game.validate().unwrap();
    assert_eq!(game.log.initial_hands.as_deref(), Some(hands.as_slice()));
}

#[test]
fn draft_seeded_game_saves_loads_and_replays_by_hash() {
    let (setup, set) = verified_setup(DraftMode::Draft77, 1357);
    let mut state = setup.start().unwrap();
    play_to_end(&mut state);
    let hands = hands_from_completed_draft(&state, &set).unwrap();
    let config = game_config(2, 99);
    let mut game = start_game_from_draft(&state, set.clone(), config.clone()).unwrap();
    advance(&mut game, 12);
    assert!(!game.log.events.is_empty());

    let hash = game.state_hash();
    let saved = game.save_json().unwrap();
    let loaded = Game::load_json(&saved).unwrap();
    assert_eq!(
        loaded.state_hash(),
        hash,
        "save/load must round-trip exactly"
    );
    assert_eq!(loaded.public.start_player, state.start_player);
    assert_eq!(
        loaded.log.initial_start_player_override,
        Some(state.start_player)
    );
    assert_hand_matches_set(&loaded, 0, &hands[0]);

    let replayed = Game::replay_with_setup_and_start_player(
        config,
        set,
        Some(hands),
        Some(state.start_player),
        &game.log.events,
    )
    .unwrap();
    assert_eq!(
        replayed.state_hash(),
        hash,
        "replay must reproduce the game"
    );
    assert_eq!(replayed.cards.hands, game.cards.hands);
}

#[test]
fn fixture_draft_seeds_two_three_and_four_player_games() {
    for (mode, players) in [
        (DraftMode::Draft77, 2usize),
        (DraftMode::Draft77, 3),
        (DraftMode::Draft10_7Simultaneous, 4),
    ] {
        let (setup, set) = fixture_setup(mode, players, 321 + players as u64);
        let mut state = setup.start().unwrap();
        play_to_end(&mut state);
        assert!(state.is_complete());
        let hands = hands_from_completed_draft(&state, &set).unwrap();
        assert_eq!(hands.len(), players);
        let game = start_game_from_draft(&state, set.clone(), game_config(players, 5)).unwrap();
        game.validate().unwrap();
        for (seat, hand) in hands.iter().enumerate() {
            assert_hand_matches_set(&game, seat, hand);
        }
        // Only the viewer's own hand is observable.
        let obs = game.observation(0).unwrap();
        assert_eq!(obs.hand, game.cards.hands[0]);
        let opponent_only: Vec<&String> = game.cards.hands[1]
            .iter()
            .filter(|id| !game.cards.hands[0].contains(id))
            .collect();
        let hand_json = serde_json::to_value(&obs)
            .unwrap()
            .get("hand")
            .unwrap()
            .to_string();
        for id in opponent_only {
            assert!(
                !hand_json.contains(id.as_str()),
                "opponent card {id} leaked into viewer 0's observation"
            );
        }
    }
}

#[test]
fn fixed_seed_reproduces_the_same_draft_seeded_game() {
    let build = || {
        // 10-7 needs 20 cards per kind and the implemented 2-player set is
        // smaller, so the fixture pool exercises the connection determinism.
        let (setup, set) = fixture_setup(DraftMode::Draft10_7Simultaneous, 2, 4242);
        let mut state = setup.start().unwrap();
        play_to_end(&mut state);
        let game = start_game_from_draft(&state, set, game_config(2, 4242)).unwrap();
        (state.final_hands(), game.state_hash())
    };
    let first = build();
    let second = build();
    assert_eq!(
        first, second,
        "fixed seed must reproduce hands and game hash"
    );
}

#[test]
fn invalid_or_incomplete_starts_are_rejected_without_state_change() {
    let (setup, set) = verified_setup(DraftMode::Draft77, 11);
    let mut state = setup.start().unwrap();
    // Incomplete draft.
    let hash = state.state_hash();
    assert!(hands_from_completed_draft(&state, &set).is_err());
    assert!(start_game_from_draft(&state, set.clone(), game_config(2, 3)).is_err());
    assert_eq!(
        state.state_hash(),
        hash,
        "rejected start must not mutate the draft"
    );
    // Player-count mismatch is rejected even for a complete draft.
    play_to_end(&mut state);
    assert!(
        start_game_from_draft(&state, set.clone(), game_config(3, 3)).is_err(),
        "a 2-player draft cannot seed a 3-player game"
    );
}

#[test]
fn fixture_cards_are_not_accepted_by_the_verified_card_set() {
    let (setup, _fixture_set) = fixture_setup(DraftMode::Draft77, 2, 55);
    let mut state = setup.start().unwrap();
    play_to_end(&mut state);
    let verified = verified_basic_card_set(2).unwrap();
    assert!(
        hands_from_completed_draft(&state, &verified).is_err(),
        "synthetic fixture ids must never pass as verified cards"
    );
    assert!(start_game_from_draft(&state, verified, game_config(2, 1)).is_err());
}

#[test]
fn explicit_hand_constructor_rejects_duplicates_and_wrong_counts() {
    let set = verified_basic_card_set(2).unwrap();
    let pool = pool_from_card_set(&set);
    let mut hand: Vec<String> = pool.occupations.iter().take(7).cloned().collect();
    hand.extend(pool.minors.iter().take(7).cloned());
    let config = game_config(2, 1);

    let mut duplicated = hand.clone();
    duplicated[1] = duplicated[0].clone();
    assert!(Game::with_cards_and_hands(
        config.clone(),
        set.clone(),
        vec![hand.clone(), duplicated]
    )
    .is_err());

    let short = hand[..10].to_vec();
    assert!(
        Game::with_cards_and_hands(config.clone(), set.clone(), vec![short, hand.clone()]).is_err()
    );

    // Seat count must equal the configured player count.
    assert!(Game::with_cards_and_hands(config, set, vec![hand]).is_err());
}

#[test]
fn normal_random_deal_still_works_and_keeps_no_initial_hands() {
    let set = verified_basic_card_set(2).unwrap();
    let game = Game::with_cards(game_config(2, 77), set).unwrap();
    game.validate().unwrap();
    assert!(game.log.initial_hands.is_none());
    assert_eq!(game.cards.hands.iter().map(Vec::len).sum::<usize>(), 28);
    let loaded = Game::load_json(&game.save_json().unwrap()).unwrap();
    assert_eq!(loaded.state_hash(), game.state_hash());
}
