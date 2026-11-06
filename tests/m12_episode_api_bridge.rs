//! M12 integration: the episode API (#148) drives the learning-data recorder.
//!
//! These tests pin the contract that `record_episode`/`record_from_episode` are
//! a faithful, observation-only replacement for a hand-written `Game` driver:
//! the same policies and seed must produce byte-for-byte the same
//! [`EpisodeRecord`], and the recorded events must replay through
//! [`Episode::replay`].
use agricola_re_ai::api::Episode;
use agricola_re_ai::cards::{verified_basic_card_set, CardSet};
use agricola_re_ai::episode_data::{
    record_episode, record_from_episode, EpisodeMeta, EpisodeRecord, TurnRecord,
};
use agricola_re_ai::evaluation::EvaluationPolicy;
use agricola_re_ai::model::{Config, Game};
use agricola_re_ai::policy::{DetailPolicy, SimpleDetails, SpacePolicy};
use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;

fn config(seed: u64, players: usize) -> Config {
    Config {
        seed,
        players,
        additional_action_space_enabled: false,
    }
}

fn meta(players: usize) -> EpisodeMeta {
    EpisodeMeta::new("bridge-sha", "none", vec!["eval".to_string(); players])
}

/// An independent `Game`-level driver, matching the historic
/// `record_self_play` implementation. Used only as the comparison reference.
fn direct_driver(config: Config, card_set: CardSet, meta: &EpisodeMeta) -> EpisodeRecord {
    let mut game = Game::with_cards(config, card_set).unwrap();
    let mut turns = Vec::new();
    for index in 0..320 {
        if game.public.finished {
            break;
        }
        let player = game.public.active_player;
        let observation = game.observation(player).unwrap();
        let legal = game.legal_spaces();
        let decision_index = game.log.events.len() as u64;
        let mut choice_rng = ChaCha8Rng::seed_from_u64(
            game.log.action_seed ^ decision_index.wrapping_mul(0x9E37_79B9_7F4A_7C15),
        );
        let space = EvaluationPolicy
            .select(&observation, &legal, &mut choice_rng)
            .unwrap();
        let action = SimpleDetails.produce(&observation, space).unwrap();
        game.apply(action).unwrap();
        let event = game.log.events.last().unwrap().clone();
        turns.push(TurnRecord {
            index,
            player,
            round: observation.public.round,
            observation,
            action: event.payload.clone(),
            before: event.before.clone(),
            after: event.after.clone(),
            reward: 0.0,
        });
    }
    assert!(game.public.finished, "reference driver must finish");
    EpisodeRecord::from_game(&game, meta, turns, None).unwrap()
}

#[test]
fn episode_recording_matches_the_direct_engine_driver() {
    for (players, seed) in [(2usize, 275u64), (3, 88), (4, 44)] {
        let card_set = CardSet::default();
        let meta = meta(players);
        let recorded = record_episode(
            config(seed, players),
            card_set.clone(),
            meta.clone(),
            &EvaluationPolicy,
            &SimpleDetails,
        )
        .unwrap();
        let reference = direct_driver(config(seed, players), card_set, &meta);
        assert_eq!(recorded, reference, "players {players} seed {seed}");
        recorded.verify().unwrap();
        assert!(recorded.finished);
        assert_eq!(recorded.rounds, 14);
        assert_eq!(recorded.turns.len(), recorded.events.len());
    }
}

#[test]
fn recorded_events_replay_through_the_episode_api() {
    let recorded = record_episode(
        config(275, 2),
        CardSet::default(),
        meta(2),
        &EvaluationPolicy,
        &SimpleDetails,
    )
    .unwrap();
    let replayed = Episode::replay(
        recorded.config.clone(),
        recorded.card_set.clone(),
        &recorded.events,
    )
    .unwrap();
    assert_eq!(replayed.state_hash(), recorded.final_state_hash);
    let result = replayed.result().unwrap();
    assert_eq!(result.standings, recorded.standings);
    // The replay produces the same first-place credit the record stored.
    let credit = result.first_credit.clone();
    let total: f64 = credit.iter().sum();
    assert!((total - 1.0).abs() < 1e-9);
}

#[test]
fn recording_rejects_an_episode_that_already_advanced() {
    let mut episode =
        Episode::reset_with_cards(config(12, 2), verified_basic_card_set(2).unwrap()).unwrap();
    let player = episode.active_player();
    let input = episode.policy_input(player).unwrap();
    let space = EvaluationPolicy
        .select(
            &input.observation,
            &input.legal_actions,
            &mut ChaCha8Rng::seed_from_u64(1),
        )
        .unwrap();
    let action = SimpleDetails.produce(&input.observation, space).unwrap();
    episode.step(player, action).unwrap();
    // A partially played episode cannot be recorded from the middle.
    let error =
        record_from_episode(&mut episode, &meta(2), &EvaluationPolicy, &SimpleDetails).unwrap_err();
    assert!(error.contains("fresh episode"), "unexpected error: {error}");
}

#[test]
fn recorded_turns_carry_only_each_actors_own_observation() {
    let card_set = verified_basic_card_set(2).unwrap();
    let recorded = record_episode(
        config(12, 2),
        card_set,
        EpisodeMeta::new("bridge-sha", "verified_basic", vec!["eval".to_string(); 2]),
        &EvaluationPolicy,
        &SimpleDetails,
    )
    .unwrap();
    for viewer in 0..2 {
        let samples = recorded.training_samples(viewer).unwrap();
        assert!(!samples.is_empty());
        for sample in samples {
            sample.assert_public().unwrap();
            assert_eq!(sample.viewer, viewer);
        }
    }
    // Every turn's observation had exactly its actor's hand length.
    for turn in &recorded.turns {
        assert_eq!(turn.observation.hand_counts.len(), 2);
        assert!(turn.observation.hand.len() <= turn.observation.hand_counts[turn.player]);
    }
}
