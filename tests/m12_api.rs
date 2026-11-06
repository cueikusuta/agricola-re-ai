//! M12 episode API exercised only through its public boundary.
//!
//! These tests do not touch `Game` internals: they use `reset / observe /
//! legal_actions / step / clone / result / replay` exactly as an external
//! evaluation or learning driver would.
use agricola_re_ai::api::Episode;
use agricola_re_ai::cards::{verified_basic_card_set, CardSet};
use agricola_re_ai::evaluation::EvaluationPolicy;
use agricola_re_ai::model::{Config, Space};
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

fn play_to_end(mut episode: Episode, mut rng: ChaCha8Rng) -> Episode {
    for _ in 0..320 {
        if episode.is_terminal() {
            return episode;
        }
        let active = episode.active_player();
        let obs = episode.observe(active).unwrap();
        let legal = episode.legal_actions(active).unwrap();
        let space = EvaluationPolicy.select(&obs, &legal, &mut rng).unwrap();
        let action = SimpleDetails.produce(&obs, space).unwrap();
        episode.step(active, action).unwrap();
    }
    panic!("episode did not finish");
}

#[test]
fn cardless_two_player_episode_replays_and_scores() {
    let episode = play_to_end(
        Episode::reset(config(275, 2)).unwrap(),
        ChaCha8Rng::seed_from_u64(1),
    );
    assert!(episode.is_terminal());
    let result = episode.result().unwrap();
    assert_eq!(result.rounds, 14);
    assert_eq!(result.first_credit.len(), 2);
    let total: f64 = result.first_credit.iter().sum();
    assert!((total - 1.0).abs() < 1e-9, "first credit must sum to 1");

    let replay = Episode::replay(
        episode.config().clone(),
        CardSet::default(),
        episode.events(),
    )
    .unwrap();
    assert_eq!(replay.state_hash(), episode.state_hash());

    // The public observation must not carry hidden decks, seeds or RNG state.
    let obs = episode.observe(0).unwrap();
    let json = serde_json::to_string(&obs).unwrap();
    for secret in [
        "hidden_round_deck",
        "round_seed",
        "action_seed",
        "delayed_well",
        "\"rng\"",
    ] {
        assert!(!json.contains(secret), "observation leaked {secret}");
    }
}

#[test]
fn four_player_episode_uses_the_same_interface() {
    let episode = play_to_end(
        Episode::reset(config(44, 4)).unwrap(),
        ChaCha8Rng::seed_from_u64(2),
    );
    assert!(episode.is_terminal());
    let result = episode.result().unwrap();
    assert_eq!(result.first_credit.len(), 4);
    assert_eq!(result.standings.rank.len(), 4);
}

#[test]
fn verified_card_episode_keeps_opponent_hands_private() {
    let cards = verified_basic_card_set(2).unwrap();
    let episode = Episode::reset_with_cards(config(12, 2), cards).unwrap();
    let obs = episode.observe(0).unwrap();
    let value: serde_json::Value = serde_json::to_value(&obs).unwrap();
    let keys: Vec<&str> = value
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    assert!(keys.contains(&"hand"));
    assert!(keys.contains(&"hand_counts"));
    assert!(keys.contains(&"card_definitions"));
    // There is no serialized field for an opponent's concrete hand.
    assert!(!keys.iter().any(|key| key.contains("opponent")));
    assert_eq!(obs.hand_counts.len(), 2);
    assert!(!obs.hand.is_empty());
}

#[test]
fn clone_is_independent() {
    let mut episode = Episode::reset(config(64, 2)).unwrap();
    let snapshot = episode.clone();
    let active = episode.active_player();
    let obs = episode.observe(active).unwrap();
    let legal: Vec<Space> = episode.legal_actions(active).unwrap();
    let space = EvaluationPolicy
        .select(&obs, &legal, &mut ChaCha8Rng::seed_from_u64(1))
        .unwrap();
    let action = SimpleDetails.produce(&obs, space).unwrap();
    episode.step(active, action).unwrap();
    assert_ne!(episode.state_hash(), snapshot.state_hash());
    assert_eq!(
        snapshot.state_hash(),
        Episode::reset(config(64, 2)).unwrap().state_hash()
    );
}
