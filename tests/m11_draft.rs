//! M11 draft foundation tests (Issue #104).
//!
//! All card IDs here are synthetic test fixtures. They are deliberately not
//! official card IDs and must never be registered as verified cards.
use agricola_re_ai::draft::{
    cpu_choice, cpu_pick, CardKind, Direction, DraftMode, DraftPhase, DraftPool, DraftRuleset,
    DraftSetup, DraftState,
};

fn pool(occ: usize, minor: usize) -> DraftPool {
    DraftPool::new(
        (0..occ).map(|i| format!("T-OCC-{i:02}")).collect(),
        (0..minor).map(|i| format!("T-MIN-{i:02}")).collect(),
    )
}

fn setup(mode: DraftMode, players: usize, seed: u64) -> DraftSetup {
    DraftSetup {
        mode,
        ruleset: DraftRuleset::explicit(
            "test-clockwise-v2",
            Direction::Right,
            Direction::Right,
            true,
        ),
        pool: pool(
            mode.dealt_per_kind() * players + 6,
            mode.dealt_per_kind() * players + 6,
        ),
        player_count: players,
        seed,
        deck_version: "fixture-v1".into(),
    }
}

/// Drive every seat with the deterministic observation-only CPU policy.
fn play_to_end(state: &mut DraftState) {
    let players = state.player_count;
    match state.mode {
        DraftMode::Deal7 => {}
        DraftMode::Choose10_7 => {
            for seat in 0..players {
                let token = state.current_token();
                let obs = state.observe(seat, token).unwrap();
                let (occ, minor) = cpu_choice(&obs).unwrap();
                state
                    .commit_choice(seat, token, occ, minor)
                    .expect("legal choice");
            }
        }
        _ => {
            let mut guard = 0;
            while !state.is_complete() {
                guard += 1;
                assert!(guard < 64, "draft did not terminate");
                let token = state.current_token();
                for seat in 0..players {
                    let obs = state.observe(seat, token).unwrap();
                    let commit = cpu_pick(&obs).unwrap();
                    state
                        .commit_pick(seat, token, commit.occupation, commit.minor)
                        .expect("legal pick");
                }
            }
        }
    }
}

#[test]
fn pool_validation_rejects_duplicates_and_short_supply() {
    let mut duplicated = pool(30, 30);
    duplicated.minors.push("T-OCC-00".into());
    assert!(duplicated.validate().is_err(), "cross-kind duplicate");

    let short = pool(10, 10);
    assert!(
        short.check_supply(DraftMode::Deal7, 2).is_err(),
        "7*2 needs 14 per kind"
    );

    let ok = pool(40, 40);
    ok.check_supply(DraftMode::Draft10_7Simultaneous, 4)
        .unwrap();
    assert!(
        short.check_supply(DraftMode::Deal7, 5).is_err(),
        "player count out of range"
    );
}

#[test]
fn deal7_gives_seven_of_each_without_selection() {
    let setup = setup(DraftMode::Deal7, 2, 7);
    let state = setup.start().unwrap();
    assert!(state.is_complete());
    assert_eq!(state.phase, DraftPhase::Complete);
    for (occ, minor) in state.final_hands() {
        assert_eq!(occ.len(), 7);
        assert_eq!(minor.len(), 7);
    }
    state.validate_invariants().unwrap();
}

#[test]
fn draft77_picks_and_passes_both_kinds_in_two_phases() {
    let setup = setup(DraftMode::Draft77, 2, 11);
    let mut state = setup.start().unwrap();
    assert_eq!(state.active_kind, Some(CardKind::Occupation));
    // Round 1: each packet starts with 7 cards.
    assert!(state
        .packets
        .iter()
        .filter(|p| p.kind == CardKind::Occupation)
        .all(|p| p.cards.len() == 7));

    // One observation round: unseen packet sizes shrink and seats swap.
    let token = state.current_token();
    let obs0 = state.observe(0, token).unwrap();
    assert_eq!(obs0.offered_occupation.len(), 7);
    let pick0 = obs0.offered_occupation[0].clone();
    state
        .commit_pick(0, token, Some(pick0.clone()), None)
        .unwrap();
    // No packet may move before every seat commits.
    assert_eq!(
        state
            .packets
            .iter()
            .find(|p| p.owner_seat == 0 && p.kind == CardKind::Occupation)
            .unwrap()
            .cards
            .len(),
        7
    );
    let obs1 = state.observe(1, token).unwrap();
    let pick1 = obs1.offered_occupation[0].clone();
    state
        .commit_pick(1, token, Some(pick1.clone()), None)
        .unwrap();
    // After both commit, the remaining 6-card packets moved one seat left.
    assert_eq!(state.round, 2);
    let p0 = state
        .packets
        .iter()
        .find(|p| p.owner_seat == 0 && p.kind == CardKind::Occupation)
        .unwrap();
    assert_eq!(p0.cards.len(), 6);
    assert!(!p0.cards.contains(&pick0) && !p0.cards.contains(&pick1));

    play_to_end(&mut state);
    assert!(state.is_complete());
    for (occ, minor) in state.final_hands() {
        assert_eq!(occ.len(), 7);
        assert_eq!(minor.len(), 7);
    }
    // 7-7 must not discard anything; occupation + minor packets are exhausted.
    assert!(state.discarded.is_empty());
    assert!(state.packets.iter().all(|p| p.cards.is_empty()));
    state.validate_invariants().unwrap();
}

#[test]
fn draft77_minor_first_also_completes_both_kinds() {
    let mut setup = setup(DraftMode::Draft77, 2, 13);
    setup.ruleset.occupation_phase_first = false;
    let mut state = setup.start().unwrap();
    assert_eq!(state.active_kind, Some(CardKind::MinorImprovement));
    play_to_end(&mut state);
    assert!(state.is_complete());
    assert_eq!(state.drafted_kinds.len(), 2);
    for (occ, minor) in state.final_hands() {
        assert_eq!(occ.len(), 7);
        assert_eq!(minor.len(), 7);
    }
}

#[test]
fn simultaneous_mode_transfers_only_after_all_commit_and_discards_three() {
    for players in [2, 3, 4] {
        let setup = setup(
            DraftMode::Draft10_7Simultaneous,
            players,
            21 + players as u64,
        );
        let mut state = setup.start().unwrap();
        assert_eq!(state.active_kind, None);
        let token = state.current_token();
        let obs0 = state.observe(0, token).unwrap();
        assert_eq!(obs0.offered_occupation.len(), 10);
        assert_eq!(obs0.offered_minor.len(), 10);
        let occ = obs0.offered_occupation[0].clone();
        let minor = obs0.offered_minor[0].clone();
        state
            .commit_pick(0, token, Some(occ.clone()), Some(minor.clone()))
            .unwrap();
        // Partial commit must not move either packet.
        let occ_owner_before = state
            .packets
            .iter()
            .find(|p| p.cards.contains(&occ))
            .unwrap()
            .owner_seat;
        assert_eq!(occ_owner_before, 0);
        // Finish the round with the remaining seats, then play the rest.
        for seat in 1..players {
            let obs = state.observe(seat, token).unwrap();
            let commit = cpu_pick(&obs).unwrap();
            state
                .commit_pick(seat, token, commit.occupation, commit.minor)
                .unwrap();
        }
        play_to_end(&mut state);
        assert!(state.is_complete());
        // 7 rounds pick 7, leaving 3 per packet for every seat and kind.
        assert_eq!(state.discarded.len(), 3 * 2 * players);
        for (occ, minor) in state.final_hands() {
            assert_eq!(occ.len(), 7);
            assert_eq!(minor.len(), 7);
        }
        state.validate_invariants().unwrap();
    }
}

#[test]
fn choose107_selects_seven_and_discards_three_privately() {
    let setup = setup(DraftMode::Choose10_7, 2, 31);
    let mut state = setup.start().unwrap();
    assert_eq!(state.phase, DraftPhase::Choosing);
    play_to_end(&mut state);
    assert!(state.is_complete());
    assert_eq!(state.discarded.len(), 3 * 2 * 2);
    for (occ, minor) in state.final_hands() {
        assert_eq!(occ.len(), 7);
        assert_eq!(minor.len(), 7);
    }
}

#[test]
fn draft_passes_clockwise_in_every_supported_player_count() {
    for players in [2, 3, 4] {
        let setup = setup(DraftMode::Draft77, players, 9000 + players as u64);
        let mut draft = setup.start().unwrap();
        assert_eq!(draft.start_player, setup.start().unwrap().start_player);
        assert!(draft.start_player < players);
        assert_eq!(
            draft
                .observe(0, draft.current_token())
                .unwrap()
                .start_player,
            draft.start_player
        );

        for seat in 0..players {
            assert_eq!(
                Direction::Right.step(seat, players),
                (seat + 1) % players,
                "clockwise must be 1P -> 2P -> ... -> 1P"
            );
        }
        let packet_id = draft
            .packets
            .iter()
            .find(|p| p.owner_seat == 0 && p.kind == CardKind::Occupation)
            .unwrap()
            .packet_id;
        let token = draft.current_token();
        for seat in 0..players {
            let choice = cpu_pick(&draft.observe(seat, token).unwrap()).unwrap();
            draft
                .commit_pick(seat, token, choice.occupation, choice.minor)
                .unwrap();
        }
        let packet = draft
            .packets
            .iter()
            .find(|p| p.packet_id == packet_id)
            .unwrap();
        assert_eq!(packet.owner_seat, 1, "seat 0 passes its packet to seat 1");
    }
}

#[test]
fn draft_starter_is_randomized_by_seed_and_old_direction_is_rejected() {
    let mut observed = std::collections::BTreeSet::new();
    for seed in 0..96 {
        observed.insert(
            setup(DraftMode::Draft77, 4, seed)
                .start()
                .unwrap()
                .start_player,
        );
    }
    assert_eq!(
        observed,
        (0usize..4).collect(),
        "every seat must be reachable"
    );
    let mut invalid = setup(DraftMode::Draft77, 4, 42);
    invalid.ruleset.direction_minor = Direction::Left;
    assert!(
        invalid.start().is_err(),
        "opposite pass directions are not adopted"
    );
}
