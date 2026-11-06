//! M11 draft observation, determinism, replay, and secrecy tests (Issue #104).
use agricola_re_ai::draft::{
    cpu_choice, cpu_pick, replay_draft, CardKind, Direction, DraftHistoryEvent, DraftMode,
    DraftPool, DraftRuleset, DraftSetup, DraftState,
};

fn pool(dealt: usize, players: usize) -> DraftPool {
    let n = dealt * players + 6;
    DraftPool::new(
        (0..n).map(|i| format!("T-OCC-{i:02}")).collect(),
        (0..n).map(|i| format!("T-MIN-{i:02}")).collect(),
    )
}

fn setup(mode: DraftMode, players: usize, seed: u64) -> DraftSetup {
    DraftSetup {
        mode,
        ruleset: DraftRuleset::explicit("test-explicit", Direction::Left, Direction::Right, true),
        pool: pool(mode.dealt_per_kind(), players),
        player_count: players,
        seed,
        deck_version: "fixture-v1".into(),
    }
}

fn play_to_end(state: &mut DraftState) {
    let players = state.player_count;
    match state.mode {
        DraftMode::Deal7 => {}
        DraftMode::Choose10_7 => {
            for seat in 0..players {
                let token = state.current_token();
                let obs = state.observe(seat, token).unwrap();
                let (occ, minor) = cpu_choice(&obs).unwrap();
                state.commit_choice(seat, token, occ, minor).unwrap();
            }
        }
        _ => {
            while !state.is_complete() {
                let token = state.current_token();
                for seat in 0..players {
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

#[test]
fn fixed_seed_is_deterministic_and_seed_sensitive() {
    for mode in [
        DraftMode::Deal7,
        DraftMode::Draft77,
        DraftMode::Draft10_7Simultaneous,
        DraftMode::Choose10_7,
    ] {
        let a = setup(mode, 3, 99).start().unwrap();
        let b = setup(mode, 3, 99).start().unwrap();
        let c = setup(mode, 3, 100).start().unwrap();
        assert_eq!(a.state_hash(), b.state_hash(), "{mode:?} same seed");
        assert_ne!(a.state_hash(), c.state_hash(), "{mode:?} different seed");
    }
}

#[test]
fn replay_from_recorded_actions_matches_hash() {
    for mode in [
        DraftMode::Draft77,
        DraftMode::Draft10_7Simultaneous,
        DraftMode::Choose10_7,
    ] {
        let setup = setup(mode, 3, 1234);
        let mut state = setup.start().unwrap();
        play_to_end(&mut state);
        assert!(state.is_complete());
        let replayed = replay_draft(&setup, &state.actions).unwrap();
        assert_eq!(
            replayed.state_hash(),
            state.state_hash(),
            "{mode:?} state hash"
        );
        assert_eq!(replayed.log_hash(), state.log_hash(), "{mode:?} log hash");
        assert_eq!(replayed.final_hands(), state.final_hands());
    }
}

#[test]
fn save_and_restore_round_trips_private_state() {
    let setup = setup(DraftMode::Draft10_7Simultaneous, 2, 55);
    let mut state = setup.start().unwrap();
    // Commit one seat, save, restore, and continue.
    let token = state.current_token();
    let obs = state.observe(0, token).unwrap();
    let commit = cpu_pick(&obs).unwrap();
    state
        .commit_pick(0, token, commit.occupation, commit.minor)
        .unwrap();
    let saved = serde_json::to_string(&state).unwrap();
    let restored: DraftState = serde_json::from_str(&saved).unwrap();
    assert_eq!(restored.state_hash(), state.state_hash());
    assert_eq!(
        restored.observe(0, token).unwrap(),
        state.observe(0, token).unwrap()
    );
    restored.validate_invariants().unwrap();
}

#[test]
fn hidden_information_does_not_change_another_viewers_observation() {
    let mut state = setup(DraftMode::Draft77, 3, 7).start().unwrap();
    let hash_before = state.state_hash();
    let before = state.observe(0, state.current_token()).unwrap();
    // Mutate only seat 1's occupation packet: replace an unseen card and the
    // server-side seed. Seat 0 must not notice anything.
    let hidden = state
        .packets
        .iter_mut()
        .find(|p| p.owner_seat == 1 && p.kind == CardKind::Occupation)
        .unwrap();
    let removed = hidden.cards[0].clone();
    hidden.cards[0] = "HIDDEN-REPLACEMENT".into();
    state.kind_of.remove(&removed);
    state
        .kind_of
        .insert("HIDDEN-REPLACEMENT".into(), CardKind::Occupation);
    state.seed = state.seed.wrapping_add(1);
    assert_ne!(state.state_hash(), hash_before, "the hidden edit is real");
    let after = state.observe(0, state.current_token()).unwrap();
    assert_eq!(before, after, "unseen hidden state must not leak");
    assert_eq!(cpu_pick(&before).unwrap(), cpu_pick(&after).unwrap());
}

#[test]
fn serialized_observation_never_contains_seed_or_unseen_picks() {
    let setup = setup(DraftMode::Draft10_7Simultaneous, 2, 4242);
    let mut state = setup.start().unwrap();
    let token = state.current_token();
    // Seat 1 picks a card from its private packet that seat 0 has never seen.
    let obs1 = state.observe(1, token).unwrap();
    let hidden_pick = obs1.offered_occupation[0].clone();
    let commit = cpu_pick(&obs1).unwrap();
    state
        .commit_pick(1, token, commit.occupation, commit.minor)
        .unwrap();
    // Complete the round so the public counts advance.
    let commit0 = cpu_pick(&state.observe(0, token).unwrap()).unwrap();
    state
        .commit_pick(0, token, commit0.occupation, commit0.minor)
        .unwrap();
    let obs0 = state.observe(0, state.current_token()).unwrap();
    let json = serde_json::to_string(&obs0).unwrap();
    assert!(!json.contains(&hidden_pick), "unseen opponent pick leaked");
    assert!(
        !json.contains(&format!("\"seed\":{}", state.seed)),
        "private seed leaked"
    );
    // The viewer still learns legitimate public counts.
    assert_eq!(obs0.hand_counts.len(), 2);
    assert_eq!(
        obs0.hand_counts[1], 2,
        "one occupation + one minor after round 1"
    );
}

#[test]
fn viewer_history_records_seen_picked_and_passed_only_for_self() {
    let setup = setup(DraftMode::Draft77, 2, 17);
    let mut state = setup.start().unwrap();
    play_to_end(&mut state);
    let history = &state.history[0];
    let seen = history
        .iter()
        .any(|e| matches!(e, DraftHistoryEvent::SeenPacket { .. }));
    let picked = history
        .iter()
        .any(|e| matches!(e, DraftHistoryEvent::OwnPick { .. }));
    let passed = history
        .iter()
        .any(|e| matches!(e, DraftHistoryEvent::PassedPacket { .. }));
    assert!(
        seen && picked && passed,
        "history must keep seen/picked/passed"
    );
    // Own picks are present, but no event may claim to be seat 1's own pick.
    let own_ids: Vec<&String> = history
        .iter()
        .filter_map(|e| match e {
            DraftHistoryEvent::OwnPick { id, .. } => Some(id),
            _ => None,
        })
        .collect();
    assert_eq!(own_ids.len(), 14, "7 occupations + 7 minors");
    let (_, minors) = &state.final_hands()[0];
    for id in minors {
        assert!(own_ids.contains(&id), "own minor {id} missing from history");
    }
}

#[test]
fn invalid_picks_are_rejected_without_state_change() {
    let setup = setup(DraftMode::Draft77, 2, 5);
    let mut state = setup.start().unwrap();
    let token = state.current_token();
    let hash = state.state_hash();

    // Card not offered to the asking seat.
    assert!(state
        .commit_pick(0, token, Some("T-OCC-999".into()), None)
        .is_err());
    // Wrong kind for the active round.
    assert!(state
        .commit_pick(0, token, None, Some("T-MIN-00".into()))
        .is_err());
    // Stale token.
    assert!(state
        .commit_pick(0, token + 99, Some("T-OCC-00".into()), None)
        .is_err());
    // Out-of-range seat.
    assert!(state
        .commit_pick(9, token, Some("T-OCC-00".into()), None)
        .is_err());
    assert_eq!(state.state_hash(), hash, "rejections must not mutate state");

    // A legal commit followed by a duplicate for the same seat fails.
    let legal = state.observe(0, token).unwrap().offered_occupation[0].clone();
    state
        .commit_pick(0, token, Some(legal.clone()), None)
        .unwrap();
    assert!(state.commit_pick(0, token, Some(legal), None).is_err());
}

#[test]
fn choice_mode_rejects_bad_selections() {
    let setup = setup(DraftMode::Choose10_7, 2, 8);
    let mut state = setup.start().unwrap();
    let token = state.current_token();
    let obs = state.observe(0, token).unwrap();
    let occ = obs.offered_occupation.clone();
    let minor = obs.offered_minor.clone();

    // Too few cards.
    assert!(state
        .commit_choice(0, token, occ[..6].to_vec(), minor[..7].to_vec())
        .is_err());
    // Duplicate card.
    let mut dup = occ[..7].to_vec();
    dup[6] = dup[0].clone();
    assert!(state
        .commit_choice(0, token, dup, minor[..7].to_vec())
        .is_err());
    // Card outside the offer.
    let mut outside = occ[..7].to_vec();
    outside[6] = "T-OCC-999".into();
    assert!(state
        .commit_choice(0, token, outside, minor[..7].to_vec())
        .is_err());
    // Picking in the choice mode is not allowed.
    assert!(state
        .commit_pick(0, token, Some(occ[0].clone()), None)
        .is_err());
}

#[test]
fn start_rejects_unconfirmed_or_insufficient_setups() {
    let mut short = setup(DraftMode::Draft10_7Simultaneous, 4, 1);
    short.pool = pool(7, 4);
    assert!(short.start().is_err(), "10*4 needs more than 7 per kind");

    let mut unnamed = setup(DraftMode::Draft77, 2, 1);
    unnamed.ruleset.id = "  ".into();
    assert!(unnamed.start().is_err(), "ruleset must be explicit");
    // No silent default direction exists.
    let _ = DraftRuleset::explicit("x", Direction::Left, Direction::Left, true);
}

#[test]
fn completed_observation_reports_no_remaining_picks_in_all_modes() {
    for mode in [
        DraftMode::Deal7,
        DraftMode::Draft77,
        DraftMode::Draft10_7Simultaneous,
        DraftMode::Choose10_7,
    ] {
        let mut state = setup(mode, 3, 505).start().unwrap();
        play_to_end(&mut state);
        assert!(state.is_complete());
        assert_eq!(
            state
                .observe(0, state.current_token())
                .unwrap()
                .remaining_to_pick,
            0,
            "{mode:?} must show no remaining picks after completion"
        );
    }
}

#[test]
fn corrupt_restored_draft_rejects_missing_cards_and_bad_packets() {
    let original = setup(DraftMode::Draft10_7Simultaneous, 3, 77)
        .start()
        .unwrap();
    let corruptions: [fn(&mut DraftState); 6] = [
        |state| {
            state.packets[0].cards.pop();
        },
        |state| {
            state.packets[0].packet_id = state.packets[1].packet_id;
        },
        |state| {
            state.packets[0].owner_seat = state.packets[1].owner_seat;
        },
        |state| {
            state.packets[0].owner_seat = state.player_count;
        },
        |state| {
            state.packets[0].cards[0] = "UNREGISTERED-CARD".into();
        },
        |state| {
            let id = state.packets[0].cards[0].clone();
            state.kind_of.insert(id, CardKind::MinorImprovement);
        },
    ];
    for (index, corrupt) in corruptions.into_iter().enumerate() {
        let mut state = original.clone();
        corrupt(&mut state);
        // The serialized form is untrusted; callers must validate it after
        // deserialization and before using it as authoritative draft state.
        let saved = serde_json::to_string(&state).unwrap();
        let restored: DraftState = serde_json::from_str(&saved).unwrap();
        assert!(
            restored.validate_invariants().is_err(),
            "corruption #{index}"
        );
    }
}
