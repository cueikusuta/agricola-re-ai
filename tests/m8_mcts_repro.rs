//! M8-3: the fixed-seed, seat-rotated comparison must be reproducible when
//! MCTS runs without a wall-clock deadline (simulation-count mode). The same
//! initial seed must produce the same action log and final state hash, so a
//! seat comparison is reproducible rather than timing-dependent.
use agricola_re_ai::mcts::MctsPolicy;
use agricola_re_ai::model::{Config, Game};
use agricola_re_ai::policy::{play_to_end, CardChoiceDetails};

fn run(seed: u64) -> (String, Vec<String>) {
    let policy = MctsPolicy {
        simulations: 1,
        tree_depth: 1,
    };
    let mut game = Game::new(Config {
        players: 2,
        seed,
        additional_action_space_enabled: false,
    });
    play_to_end(&mut game, &policy, &CardChoiceDetails).expect("MCTS game finishes");
    let actions = game
        .log
        .events
        .iter()
        .map(|event| event.payload.to_string())
        .collect();
    (game.state_hash(), actions)
}

#[test]
fn deterministic_mcts_seat_rotation_is_reproducible() {
    let first = run(4242);
    let second = run(4242);
    assert_eq!(
        first, second,
        "same seed must reproduce the same actions and final state"
    );
    assert!(!first.1.is_empty(), "a full game must log actions");
}

#[test]
fn different_seeds_produce_different_games() {
    assert_ne!(run(4242).0, run(4243).0);
}
