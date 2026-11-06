//! M12 episode/learning-data foundation tests.
//!
//! The data layer must replay its own records, reject corruption and duplicate
//! seeds, and never expose another player's private information.
use agricola_re_ai::cards::CardSet;
use agricola_re_ai::draft::{
    cpu_choice, cpu_pick, Direction, DraftMode, DraftPool, DraftRuleset, DraftSetup,
};
use agricola_re_ai::episode_data::{
    from_jsonl, inspect_dataset, record_self_play, to_jsonl, DraftEpisode, EpisodeMeta,
    EpisodeRecord,
};
use agricola_re_ai::evaluation::EvaluationPolicy;
use agricola_re_ai::model::Config;
use agricola_re_ai::policy::SimpleDetails;

fn meta(players: usize) -> EpisodeMeta {
    EpisodeMeta::new("test-sha", "none", vec!["eval".to_string(); players])
}

fn config(players: usize, seed: u64) -> Config {
    Config {
        players,
        seed,
        additional_action_space_enabled: false,
    }
}

fn self_play_record(players: usize, seed: u64) -> EpisodeRecord {
    record_self_play(
        config(players, seed),
        CardSet::default(),
        meta(players),
        &EvaluationPolicy,
        &SimpleDetails,
    )
    .expect("self-play recording")
}

fn completed_draft(
    mode: DraftMode,
    players: usize,
    seed: u64,
) -> agricola_re_ai::draft::DraftState {
    let n = mode.dealt_per_kind() * players;
    let setup = DraftSetup {
        mode,
        ruleset: DraftRuleset::explicit("test-explicit", Direction::Left, Direction::Right, true),
        pool: DraftPool::new(
            (0..n).map(|i| format!("FIX-OCC-{i:02}")).collect(),
            (0..n).map(|i| format!("FIX-MIN-{i:02}")).collect(),
        ),
        player_count: players,
        seed,
        deck_version: "fixture-v1".into(),
    };
    let mut state = setup.start().unwrap();
    match mode {
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
    state
}

#[test]
fn recorded_self_play_verifies_and_replays_for_two_three_and_four_players() {
    for (players, seed) in [(2usize, 275u64), (3, 88), (4, 44)] {
        let record = self_play_record(players, seed);
        record.verify().unwrap();
        assert!(record.finished);
        assert_eq!(record.rounds, 14);
        assert_eq!(record.config.players, players);
        assert_eq!(record.turns.len(), record.events.len());
        assert!(record.turns.iter().all(|turn| turn.player < players));
        // Fresh recording of the same seed reproduces the same final hash.
        assert_eq!(
            record.final_state_hash,
            self_play_record(players, seed).final_state_hash
        );
    }
}

#[test]
fn jsonl_round_trips_and_reports_the_bad_line() {
    let records = vec![self_play_record(2, 1), self_play_record(2, 2)];
    let text = to_jsonl(&records).unwrap();
    assert_eq!(text.lines().count(), 2);
    let parsed = from_jsonl(&text).unwrap();
    assert_eq!(parsed, records);

    // A truncated final line is reported with its line number.
    let mut broken = text.clone();
    broken.truncate(broken.trim_end().len() - 5);
    let error = from_jsonl(&broken).unwrap_err();
    assert!(error.starts_with("line 2:"), "unexpected error: {error}");

    assert!(from_jsonl("").is_err());
    assert!(from_jsonl("\n\n").is_err());
}

#[test]
fn dataset_report_detects_duplicate_seeds_and_corruption() {
    let clean = vec![self_play_record(2, 10), self_play_record(2, 11)];
    let report = inspect_dataset(&clean);
    assert!(report.is_clean(), "{report:?}");
    assert_eq!(report.total, 2);
    assert_eq!(report.finished, 2);
    assert_eq!(report.player_counts, [2].into_iter().collect());

    // Same seed twice is reported, even though both records are valid.
    let duplicated = vec![self_play_record(2, 10), self_play_record(2, 10)];
    let report = inspect_dataset(&duplicated);
    assert_eq!(report.duplicate_seeds, vec![10]);
    assert!(!report.is_clean());

    // A tampered final hash fails verification.
    let mut corrupted = self_play_record(2, 12);
    corrupted.final_state_hash = "0".repeat(64);
    let report = inspect_dataset(&[corrupted]);
    assert_eq!(report.corrupt.len(), 1);
    assert!(!report.is_clean());
}

#[test]
fn verify_detects_all_recorded_tampering() {
    let record = self_play_record(2, 2024);
    record.verify().expect("the untouched record is valid");

    // 1. A corrupted learning reward (including NaN) is rejected.
    let mut reward_tampered = record.clone();
    reward_tampered.turns[0].reward += 1.0;
    assert!(reward_tampered.verify().is_err());
    let mut nan_reward = record.clone();
    nan_reward.turns[0].reward = f64::NAN;
    assert!(nan_reward.verify().is_err());
    let mut infinite_reward = record.clone();
    infinite_reward.turns[0].reward = f64::INFINITY;
    assert!(infinite_reward.verify().is_err());

    // 2. A non-sequential turn index is rejected.
    let mut index_tampered = record.clone();
    index_tampered.turns[1].index = 99;
    assert!(index_tampered.verify().is_err());

    // 3. Editing the terminal finished flag is rejected.
    let mut finished_tampered = record.clone();
    finished_tampered.finished = !finished_tampered.finished;
    assert!(finished_tampered.verify().is_err());

    // 4. Editing the terminal round count is rejected.
    let mut rounds_tampered = record.clone();
    rounds_tampered.rounds = rounds_tampered.rounds.wrapping_sub(1);
    assert!(rounds_tampered.verify().is_err());

    // 5. Editing the recorded action payload is corruption even though the
    //    event log and its before/after hashes are untouched.
    let mut action_tampered = record.clone();
    action_tampered.turns[0].action = serde_json::json!({"tampered": true});
    assert!(action_tampered.verify().is_err());
    assert_eq!(inspect_dataset(&[action_tampered]).corrupt.len(), 1);

    // 6. Substituting another seat's observation must be rejected.
    let mut observation_tampered = record.clone();
    let player = observation_tampered.turns[0].observation.player_id;
    observation_tampered.turns[0].observation.player_id = (player + 1) % 2;
    assert!(observation_tampered.verify().is_err());

    // Two individually well-formed observations exchanged between turns are
    // detected because each is checked against the live replayed state.
    let last = record.turns.len() - 1;
    let first_view = record.turns[0].observation.clone();
    let last_view = record.turns[last].observation.clone();
    assert_ne!(first_view, last_view, "turns must observe different states");
    let mut swapped = record.clone();
    swapped.turns[0].observation = last_view;
    swapped.turns[last].observation = first_view;
    assert!(swapped.verify().is_err());

    // 7. A mismatched initial state hash is rejected.
    let mut initial_tampered = record.clone();
    initial_tampered.initial_state_hash = "0".repeat(64);
    assert!(initial_tampered.verify().is_err());

    // A wrong round label on a turn is rejected.
    let mut round_tampered = record.clone();
    round_tampered.turns[0].round = round_tampered.turns[0].round.wrapping_add(1);
    assert!(round_tampered.verify().is_err());
}

#[test]
fn training_samples_reject_a_corrupted_record() {
    let record = self_play_record(2, 3030);
    assert!(!record.training_samples(0).unwrap().is_empty());

    // 8. A tampered record must never be converted into learning data.
    let mut reward_tampered = record.clone();
    reward_tampered.turns[0].reward = f64::NAN;
    assert!(reward_tampered.training_samples(0).is_err());

    let mut index_tampered = record.clone();
    index_tampered.turns[1].index = 99;
    assert!(index_tampered.training_samples(0).is_err());

    let mut observation_tampered = record.clone();
    let player = observation_tampered.turns[0].observation.player_id;
    observation_tampered.turns[0].observation.player_id = (player + 1) % 2;
    assert!(observation_tampered.training_samples(0).is_err());

    // An out-of-range viewer is still rejected on a valid record.
    assert!(record.training_samples(2).is_err());
}

#[test]
fn dataset_report_summarizes_provenance_seed_range_and_completeness() {
    let mut records = vec![self_play_record(2, 10), self_play_record(2, 11)];
    let report = inspect_dataset(&records);
    assert!(report.is_complete());
    assert!(report.has_uniform_provenance());
    assert_eq!(report.seed_min, Some(10));
    assert_eq!(report.seed_max, Some(11));
    assert_eq!(report.distinct_seeds, 2);
    assert_eq!(report.total_turns, report.total_events);
    assert_eq!(
        report.formats,
        ["agricola-episode".to_string()].into_iter().collect()
    );
    assert_eq!(
        report.card_modes,
        ["none".to_string()].into_iter().collect()
    );
    assert_eq!(
        report.code_shas,
        ["test-sha".to_string()].into_iter().collect()
    );

    // A different producing revision must not look like one uniform dataset.
    records[1].code_sha = "other-sha".into();
    let mixed = inspect_dataset(&records);
    assert!(!mixed.has_uniform_provenance());
    assert_eq!(mixed.code_shas.len(), 2);

    // An empty record set is not a complete dataset.
    let empty = inspect_dataset(&[]);
    assert!(!empty.is_complete());
    assert!(empty.seed_min.is_none());
    assert_eq!(empty.distinct_seeds, 0);
}

#[test]
fn training_samples_expose_only_the_viewers_own_turns() {
    let record = self_play_record(3, 303);
    for viewer in 0..3 {
        let samples = record.training_samples(viewer).unwrap();
        let acting_turns: Vec<_> = record
            .turns
            .iter()
            .filter(|turn| turn.player == viewer)
            .collect();
        assert_eq!(samples.len(), acting_turns.len());
        assert!(!samples.is_empty());
        for (sample, turn) in samples.iter().zip(acting_turns) {
            assert_eq!(sample.viewer, viewer);
            assert_eq!(sample.observation, turn.observation);
            assert_eq!(sample.observation.hand, turn.observation.hand);
            assert_eq!(sample.action, turn.action);
            sample.assert_public().unwrap();
        }
    }
    assert!(record.training_samples(3).is_err());
}

#[test]
fn draft_history_is_kept_per_viewer_only() {
    let mut record = self_play_record(2, 515);
    let state = completed_draft(DraftMode::Draft77, 2, 515);
    let draft = DraftEpisode::from_state(&state, "test_fixture").unwrap();
    assert_eq!(draft.views.len(), 2);
    // Completion exposes each seat's own picks and no offered cards.
    for (seat, view) in draft.views.iter().enumerate() {
        assert!(view.offered_occupation.is_empty());
        assert!(view.offered_minor.is_empty());
        let (occupations, minors) = &state.final_hands()[seat];
        let own: Vec<&String> = occupations.iter().chain(minors.iter()).collect();
        assert_eq!(view.own_picked.len(), 14);
        assert!(own.iter().all(|id| view.own_picked.contains(id)));
    }
    // Seat 1's cards are absent from seat 0's own picks.
    for id in &state.final_hands()[1].0 {
        assert!(!draft.views[0].own_picked.contains(id));
    }
    record.draft = Some(draft.clone());
    record.verify().unwrap();

    let samples = record.training_samples(0).unwrap();
    assert!(samples[0].draft.is_some());
    assert_eq!(
        samples[0].draft.as_ref().unwrap().own_picked,
        draft.views[0].own_picked
    );
    let history = record.draft_history(0).unwrap();
    assert!(!history.is_empty());
}
