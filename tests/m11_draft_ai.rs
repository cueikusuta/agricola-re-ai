//! M11 draft AI baseline tests (#106).
//!
//! The baselines must be reproducible, use only `DraftObservation`, complete
//! every draft mode, and degrade to a legal pick when the public catalog is
//! missing. Real playing strength is *not* measured here.
use agricola_re_ai::draft::{
    CardKind, Direction, DraftCardInfo, DraftMode, DraftObservation, DraftPhase, DraftPool,
    DraftRuleset, DraftSetup,
};
use agricola_re_ai::draft_ai::{
    compare_draft_ais, decision_rng, run_draft, DraftAiKind, DraftDecision,
};
use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;

fn fixture_setup(mode: DraftMode, players: usize, seed: u64) -> DraftSetup {
    let n = mode.dealt_per_kind() * players;
    let pool = DraftPool::new(
        (0..n).map(|i| format!("FIX-OCC-{i:02}")).collect(),
        (0..n).map(|i| format!("FIX-MIN-{i:02}")).collect(),
    );
    DraftSetup {
        mode,
        ruleset: DraftRuleset::explicit(
            "test-clockwise-v2",
            Direction::Right,
            Direction::Right,
            true,
        ),
        pool,
        player_count: players,
        seed,
        deck_version: "fixture-v1".into(),
    }
}

fn catalog_for(setup: &DraftSetup) -> Vec<DraftCardInfo> {
    let info = |id: &String, kind: CardKind| DraftCardInfo {
        id: id.clone(),
        kind,
        points: 1,
        cost: vec![],
        effect_tags: vec!["on_play_gain".into()],
    };
    setup
        .pool
        .occupations
        .iter()
        .map(|id| info(id, CardKind::Occupation))
        .chain(
            setup
                .pool
                .minors
                .iter()
                .map(|id| info(id, CardKind::MinorImprovement)),
        )
        .collect()
}

fn observation(
    offered_occupation: Vec<&str>,
    offered_minor: Vec<&str>,
    own_picked: Vec<&str>,
    catalog: Vec<DraftCardInfo>,
) -> DraftObservation {
    DraftObservation {
        viewer: 0,
        start_player: 0,
        mode: DraftMode::Draft77,
        ruleset_id: "test-explicit".into(),
        phase: DraftPhase::Drafting,
        round: 1,
        active_kind: Some(CardKind::Occupation),
        offered_occupation: offered_occupation.into_iter().map(str::to_owned).collect(),
        offered_minor: offered_minor.into_iter().map(str::to_owned).collect(),
        own_picked: own_picked.into_iter().map(str::to_owned).collect(),
        own_observation_history: vec![],
        hand_counts: vec![0],
        remaining_to_pick: 7,
        request_token: 0,
        catalog,
    }
}

fn card(id: &str, kind: CardKind, points: i32, tags: &[&str]) -> DraftCardInfo {
    DraftCardInfo {
        id: id.into(),
        kind,
        points,
        cost: vec![],
        effect_tags: tags.iter().map(|tag| (*tag).to_owned()).collect(),
    }
}

#[test]
fn every_baseline_completes_every_mode() {
    for mode in [
        DraftMode::Deal7,
        DraftMode::Draft77,
        DraftMode::Draft10_7Simultaneous,
        DraftMode::Choose10_7,
    ] {
        let setup = fixture_setup(mode, 3, 77);
        let catalog = catalog_for(&setup);
        for ai in DraftAiKind::all() {
            let seat_ais = vec![ai; 3];
            let state = run_draft(&setup, &seat_ais, 77, &catalog).unwrap();
            assert!(state.is_complete(), "{mode:?} {ai:?} did not finish");
            for (occupations, minors) in state.final_hands() {
                assert_eq!(occupations.len(), 7, "{mode:?} {ai:?}");
                assert_eq!(minors.len(), 7, "{mode:?} {ai:?}");
            }
        }
    }
}

#[test]
fn fixed_seed_reproduces_the_same_draft() {
    let setup = fixture_setup(DraftMode::Draft77, 3, 2024);
    let catalog = catalog_for(&setup);
    for ai in DraftAiKind::all() {
        let seat_ais = vec![ai; 3];
        let first = run_draft(&setup, &seat_ais, 2024, &catalog).unwrap();
        let second = run_draft(&setup, &seat_ais, 2024, &catalog).unwrap();
        assert_eq!(
            first.state_hash(),
            second.state_hash(),
            "{ai:?} determinism"
        );
    }
    // A different seed must change the Random baseline's choices.
    let random = vec![DraftAiKind::Random; 3];
    let other = run_draft(&setup, &random, 9999, &catalog).unwrap();
    let base = run_draft(&setup, &random, 2024, &catalog).unwrap();
    assert_ne!(base.state_hash(), other.state_hash());
}

#[test]
fn decisions_do_not_change_when_only_hidden_state_changes() {
    let setup = fixture_setup(DraftMode::Draft77, 3, 17);
    let catalog = catalog_for(&setup);
    let mut state = setup.start().unwrap();
    let token = state.current_token();
    let before: Vec<DraftDecision> = DraftAiKind::all()
        .iter()
        .map(|&ai| {
            let obs = state.observe_with_catalog(0, token, &catalog).unwrap();
            ai.decide_pick(&obs, &mut decision_rng(17, 0, token, ai))
                .unwrap()
        })
        .collect();

    // Change only seat 1's occupation packet and the private seed.
    let hidden = state
        .packets
        .iter_mut()
        .find(|packet| packet.owner_seat == 1 && packet.kind == CardKind::Occupation)
        .unwrap();
    let removed = hidden.cards[0].clone();
    hidden.cards[0] = "HIDDEN-REPLACEMENT".into();
    state.kind_of.remove(&removed);
    state
        .kind_of
        .insert("HIDDEN-REPLACEMENT".into(), CardKind::Occupation);
    state.seed = state.seed.wrapping_add(1);

    let after: Vec<DraftDecision> = DraftAiKind::all()
        .iter()
        .map(|&ai| {
            let obs = state.observe_with_catalog(0, token, &catalog).unwrap();
            ai.decide_pick(&obs, &mut decision_rng(17, 0, token, ai))
                .unwrap()
        })
        .collect();
    assert_eq!(
        before, after,
        "unseen hidden state must not change a decision"
    );
}

#[test]
fn card_value_prefers_declared_points_and_returns_only_offered_cards() {
    let offered: Vec<String> = (0..10).map(|i| format!("O{i}")).collect();
    let catalog: Vec<DraftCardInfo> = (0..10)
        .map(|i| card(&format!("O{i}"), CardKind::Occupation, i, &[]))
        .collect();
    let mut obs = observation(vec![], vec![], vec![], catalog);
    obs.mode = DraftMode::Choose10_7;
    obs.phase = DraftPhase::Choosing;
    obs.offered_occupation = offered;
    obs.offered_minor = (0..10).map(|i| format!("M{i}")).collect();
    obs.catalog
        .extend((0..10).map(|i| card(&format!("M{i}"), CardKind::MinorImprovement, i, &[])));

    let decision = DraftAiKind::CardValue
        .decide_choice(&obs, &mut ChaCha8Rng::seed_from_u64(1))
        .unwrap();
    // Ranked best-first, the seven highest declared-point occupations are
    // O9..O3.
    let expected: Vec<String> = (3..10).rev().map(|i| format!("O{i}")).collect();
    assert_eq!(decision.occupations, expected);
    // Diagnostics only ever name offered cards.
    assert!(decision
        .ranked_occupations
        .iter()
        .all(|scored| obs.offered_occupation.contains(&scored.id)));
}

#[test]
fn synergy_prefers_affinity_with_already_picked_cards() {
    // A and B have equal base value. The viewer already holds three cards with
    // tag "x", so Synergy must prefer B while CardValue keeps the offer order.
    let catalog = vec![
        card("A", CardKind::Occupation, 0, &["y"]),
        card("B", CardKind::Occupation, 0, &["x"]),
        card("C1", CardKind::Occupation, 0, &["x"]),
        card("C2", CardKind::Occupation, 0, &["x"]),
        card("C3", CardKind::Occupation, 0, &["x"]),
    ];
    let obs = observation(vec!["A", "B"], vec![], vec!["C1", "C2", "C3"], catalog);
    let mut rng = ChaCha8Rng::seed_from_u64(5);
    let value_pick = DraftAiKind::CardValue.decide_pick(&obs, &mut rng).unwrap();
    assert_eq!(value_pick.occupation.as_deref(), Some("A"));
    let synergy_pick = DraftAiKind::Synergy.decide_pick(&obs, &mut rng).unwrap();
    assert_eq!(synergy_pick.occupation.as_deref(), Some("B"));
}

#[test]
fn missing_catalog_falls_back_to_a_legal_pick() {
    let obs = observation(vec!["A", "B"], vec![], vec![], vec![]);
    for ai in DraftAiKind::all() {
        let decision = ai
            .decide_pick(&obs, &mut ChaCha8Rng::seed_from_u64(3))
            .unwrap();
        let picked = decision.occupation.expect("a legal occupation");
        assert!(
            obs.offered_occupation.contains(&picked),
            "{ai:?} picked a card outside the offer"
        );
    }
}

#[test]
fn comparison_covers_all_ais_and_seats_deterministically() {
    let setup = fixture_setup(DraftMode::Draft77, 3, 4242);
    let catalog = catalog_for(&setup);
    let first = compare_draft_ais(&setup, &catalog, 4242).unwrap();
    let second = compare_draft_ais(&setup, &catalog, 4242).unwrap();
    assert_eq!(first, second, "comparison must be reproducible");
    assert_eq!(first.len(), DraftAiKind::all().len() * 3);
    for outcome in &first {
        assert_eq!(outcome.occupations.len(), 7);
        assert_eq!(outcome.minors.len(), 7);
    }
    // Every (ai, seat) pair is present exactly once.
    for ai in DraftAiKind::all() {
        for seat in 0..3 {
            assert_eq!(
                first
                    .iter()
                    .filter(|outcome| outcome.ai == ai && outcome.seat == seat)
                    .count(),
                1
            );
        }
    }
}
