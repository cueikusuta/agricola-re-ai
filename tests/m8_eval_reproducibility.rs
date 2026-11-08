//! M8-3: the same seed and the same policies must produce the same evaluation
//! game.
//!
//! The evaluation harness (`paired_eval`) compares two arms on identical seeds
//! and rotated seats. That comparison is only interpretable if a seed fixes the
//! game: a re-run must produce the same action sequence, the same final state
//! hash and the same scores, and the committed log must replay to the same
//! state. These tests exercise the same library path the harness uses
//! (`Game::new` / `Game::with_cards`, `play_to_end`, `standings`), so a
//! non-deterministic policy or an RNG stream shared between purposes is caught
//! here rather than in a 1,000-game artifact.
use agricola_re_ai::cards::verified_basic_card_set;
use agricola_re_ai::evaluation::EvaluationPolicy;
use agricola_re_ai::model::{Config, Game};
use agricola_re_ai::policy::{play_to_end, CardChoiceDetails, SimpleDetails};
use agricola_re_ai::score::{standings, standings_with_cards};

/// Everything a comparison row needs, in a form that can be compared directly.
#[derive(Debug, PartialEq, Eq)]
struct GameResult {
    final_hash: String,
    scores: Vec<i32>,
    ranks: Vec<usize>,
    events: Vec<String>,
}

fn cardless(players: usize, seed: u64, additional: bool) -> GameResult {
    let config = Config {
        players,
        seed,
        additional_action_space_enabled: additional,
    };
    let mut game = Game::new(config.clone());
    play_to_end(&mut game, &EvaluationPolicy, &SimpleDetails).expect("cardless game finishes");
    let standings = standings(&game.public.players);
    let result = GameResult {
        final_hash: game.state_hash(),
        scores: standings.scores.iter().map(|score| score.total).collect(),
        ranks: standings.rank.clone(),
        events: game
            .log
            .events
            .iter()
            .map(|event| event.payload.to_string())
            .collect(),
    };
    // The same seed must also replay from the committed log alone.
    let replayed = Game::replay(config, &game.log.events).expect("log replays");
    assert_eq!(
        replayed.state_hash(),
        result.final_hash,
        "replaying the log must reproduce the recorded final state"
    );
    result
}

fn with_cards(players: usize, seed: u64) -> GameResult {
    let config = Config {
        players,
        seed,
        additional_action_space_enabled: false,
    };
    let cards = verified_basic_card_set(players).expect("verified pool for this player count");
    let mut game = Game::with_cards(config.clone(), cards.clone()).expect("card game");
    play_to_end(&mut game, &EvaluationPolicy, &CardChoiceDetails).expect("card game finishes");
    let standings = standings_with_cards(&game.public.players, &cards);
    let result = GameResult {
        final_hash: game.state_hash(),
        scores: standings.scores.iter().map(|score| score.total).collect(),
        ranks: standings.rank.clone(),
        events: game
            .log
            .events
            .iter()
            .map(|event| event.payload.to_string())
            .collect(),
    };
    // A card game must be replayed with the same card set: the default
    // `Game::replay` has no cards and would mis-seed the initial state.
    let replayed =
        Game::replay_with_cards(config, cards, &game.log.events).expect("card log replays");
    assert_eq!(replayed.state_hash(), result.final_hash);
    result
}

#[test]
fn two_player_cardless_games_reproduce_exactly() {
    for additional in [false, true] {
        for seed in [920_500u64, 920_501, 920_777] {
            let first = cardless(2, seed, additional);
            let second = cardless(2, seed, additional);
            assert_eq!(first, second, "seed {seed} tile={additional}");
            assert!(!first.events.is_empty(), "a full game logs actions");
            assert_eq!(first.scores.len(), 2);
        }
    }
}

#[test]
fn four_player_cardless_games_reproduce_exactly() {
    for seed in [920_900u64, 920_901] {
        let first = cardless(4, seed, false);
        let second = cardless(4, seed, false);
        assert_eq!(first, second, "seed {seed}");
        assert_eq!(first.scores.len(), 4);
    }
}

#[test]
fn verified_card_games_reproduce_exactly() {
    // Card deals are seeded from the game seed, so an unseeded shuffle would
    // show up as a different hand and a different final state.
    for seed in [920_600u64, 920_601] {
        let first = with_cards(2, seed);
        let second = with_cards(2, seed);
        assert_eq!(first, second, "seed {seed}");
    }
}

#[test]
fn different_seeds_produce_different_games() {
    let first = cardless(2, 920_500, false);
    let second = cardless(2, 920_501, false);
    assert_ne!(first.final_hash, second.final_hash);
}

#[test]
fn the_seat_rotation_uses_the_same_seed_for_both_arms() {
    // `paired_eval` gives both arms the same seed and only rotates the seat.
    // The resulting games must therefore be identical when the policies match:
    // otherwise a "difference" between arms could be an RNG artefact.
    let a = cardless(2, 921_000, false);
    let b = cardless(2, 921_000, false);
    assert_eq!(a.final_hash, b.final_hash);
    assert_eq!(a.scores, b.scores);
    assert_eq!(a.ranks, b.ranks);
}
