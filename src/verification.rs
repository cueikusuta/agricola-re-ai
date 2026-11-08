//! Whole-mode simulator verification (Issue #108 / #110 follow-up).
//!
//! `--batch` only aggregates final scores, so it cannot show that a game
//! actually replayed, that its invariants held after every action, that a saved
//! game reloads to the same state, that observations stay private, or that a
//! seed is reproducible. This module runs those checks for a single game and
//! returns machine-readable evidence, so a mass run can fail on any of them
//! instead of only on a non-zero exit status.
//!
//! What each check proves, and what it does not:
//!
//! - `completed_fourteen_rounds`: the engine reached round 14 and marked the
//!   game finished. It does not prove that the rules are correct.
//! - `final_state_invariants`: `Game::validate` passes on the final state.
//! - `replay_matches_log`: `Game::replay` re-applies every recorded event and
//!   checks the before/after state hash. `apply_recorded` calls
//!   `Game::validate` after each action, so this is the per-action invariant
//!   check as well as the replay check.
//! - `save_restore_round_trip`: `save_json` -> `load_json` (which itself
//!   re-verifies the event chain) returns the same state hash.
//! - `observation_structural_privacy`: each viewer's `Observation` carries
//!   exactly their own hand, and the serialised observation contains no hidden
//!   key (`hidden_round_deck`, `undealt`, `rng`, `hands`). In the cardless modes
//!   verified here every hand is empty, so this check is a **structural**
//!   privacy check; non-empty hand secrecy is covered by the card-mode fixture
//!   test in this module and by the card-mode tests (#108/#110).
//! - `seed_reproducible`: the same config and policy produce the same final
//!   state hash when the game is played again.
//!
//! Cardless games only: card-mode coverage is gated by the M10 verified pool,
//! and this module must not imply that an arbitrary card set is verified.

use crate::evaluation::EvaluationPolicy;
use crate::model::{Config, Game, Observation};
use crate::policy::{
    play_to_end, DetailPolicy, GreedyPolicy, RandomPolicy, SimpleDetails, SpacePolicy,
};
use crate::score::standings;
use serde::Serialize;
use std::time::Instant;

pub const VERIFICATION_SCHEMA_VERSION: u32 = 1;

static RANDOM_POLICY: RandomPolicy = RandomPolicy;
static GREEDY_POLICY: GreedyPolicy = GreedyPolicy;
static EVALUATION_POLICY: EvaluationPolicy = EvaluationPolicy;

/// The space policy used for every seat of a verification game.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum VerifiedPolicy {
    Random,
    Greedy,
    Eval,
}

impl VerifiedPolicy {
    pub fn parse(value: &str) -> Result<Self, String> {
        match value {
            "random" => Ok(Self::Random),
            "greedy" => Ok(Self::Greedy),
            "eval" => Ok(Self::Eval),
            other => Err(format!(
                "unknown policy {other:?}; expected random, greedy or eval"
            )),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Random => "random",
            Self::Greedy => "greedy",
            Self::Eval => "eval",
        }
    }

    fn space_policy(self) -> &'static dyn SpacePolicy {
        match self {
            Self::Random => &RANDOM_POLICY,
            Self::Greedy => &GREEDY_POLICY,
            Self::Eval => &EVALUATION_POLICY,
        }
    }
}

/// The inclusive `(first, last)` seed range of a verification run.
///
/// Sharding assigns seeds by index, so an overflowing end would wrap in a
/// release build and repeat seeds across a mass run. Callers validate this once,
/// before any game is played.
pub fn seed_range(seed_start: u64, seeds: usize) -> Result<(u64, u64), String> {
    if seeds == 0 {
        return Err("seeds must be greater than zero".into());
    }
    let last_offset = u64::try_from(seeds - 1).map_err(|_| "seed count does not fit in u64")?;
    let last = seed_start
        .checked_add(last_offset)
        .ok_or("seed range overflows u64")?;
    Ok((seed_start, last))
}

/// A supported cardless mode: player count and additional-tile setting.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct ModeSpec {
    pub players: usize,
    pub additional_action_space_enabled: bool,
}

impl ModeSpec {
    pub fn new(players: usize, additional_action_space_enabled: bool) -> Result<Self, String> {
        if !(2..=4).contains(&players) {
            return Err("players must be 2, 3 or 4".into());
        }
        Ok(Self {
            players,
            additional_action_space_enabled,
        })
    }

    fn config(self, seed: u64) -> Config {
        Config {
            seed,
            players: self.players,
            additional_action_space_enabled: self.additional_action_space_enabled,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct VerificationCheck {
    pub name: &'static str,
    pub passed: bool,
    /// Human-readable cause when the check fails; "ok" otherwise.
    pub detail: String,
}

impl VerificationCheck {
    fn pass(name: &'static str) -> Self {
        Self {
            name,
            passed: true,
            detail: "ok".into(),
        }
    }

    fn fail(name: &'static str, detail: impl Into<String>) -> Self {
        Self {
            name,
            passed: false,
            detail: detail.into(),
        }
    }
}

/// Machine-readable result for one game.
#[derive(Clone, Debug, Serialize)]
pub struct GameReport {
    pub schema_version: u32,
    pub seed: u64,
    pub players: usize,
    pub additional_action_space_enabled: bool,
    pub policy: VerifiedPolicy,
    pub finished: bool,
    pub rounds_completed: u8,
    pub actions: usize,
    pub state_hash: Option<String>,
    pub scores: Vec<i32>,
    pub ranks: Vec<usize>,
    pub elapsed_millis: u64,
    pub checks: Vec<VerificationCheck>,
}

impl GameReport {
    pub fn passed(&self) -> bool {
        self.checks.iter().all(|check| check.passed)
    }

    pub fn failed_check_names(&self) -> Vec<&'static str> {
        self.checks
            .iter()
            .filter(|check| !check.passed)
            .map(|check| check.name)
            .collect()
    }
}

/// Play one cardless game and run every verification check on it.
///
/// The returned report always contains a `completed_fourteen_rounds` entry, so
/// a play-through error is reported as a failed check instead of a panic. The
/// remaining checks are only meaningful once a game has been played, and are
/// recorded as failures when it has not.
pub fn verify_game(mode: ModeSpec, seed: u64, policy: VerifiedPolicy) -> GameReport {
    let started = Instant::now();
    let config = mode.config(seed);
    let mut report = GameReport {
        schema_version: VERIFICATION_SCHEMA_VERSION,
        seed,
        players: mode.players,
        additional_action_space_enabled: mode.additional_action_space_enabled,
        policy,
        finished: false,
        rounds_completed: 0,
        actions: 0,
        state_hash: None,
        scores: Vec::new(),
        ranks: Vec::new(),
        elapsed_millis: 0,
        checks: Vec::new(),
    };
    let game = match play(&config, policy) {
        Ok(game) => game,
        Err(error) => {
            report
                .checks
                .push(VerificationCheck::fail("completed_fourteen_rounds", error));
            report.elapsed_millis = started.elapsed().as_millis() as u64;
            return report;
        }
    };
    report.finished = game.public.finished;
    report.rounds_completed = game.public.round;
    report.actions = game.log.events.len();
    report.state_hash = Some(game.state_hash());
    let standings = standings(&game.public.players);
    report.scores = standings.scores.iter().map(|score| score.total).collect();
    report.ranks = standings.rank.clone();

    report.checks.push(match completed_check(&game) {
        Ok(()) => VerificationCheck::pass("completed_fourteen_rounds"),
        Err(error) => VerificationCheck::fail("completed_fourteen_rounds", error),
    });
    report.checks.push(match invariants_check(&game) {
        Ok(()) => VerificationCheck::pass("final_state_invariants"),
        Err(error) => VerificationCheck::fail("final_state_invariants", error),
    });
    report.checks.push(match replay_check(&config, &game) {
        Ok(()) => VerificationCheck::pass("replay_matches_log"),
        Err(error) => VerificationCheck::fail("replay_matches_log", error),
    });
    report.checks.push(match save_restore_check(&game) {
        Ok(()) => VerificationCheck::pass("save_restore_round_trip"),
        Err(error) => VerificationCheck::fail("save_restore_round_trip", error),
    });
    report.checks.push(match privacy_check(&game) {
        Ok(()) => VerificationCheck::pass("observation_structural_privacy"),
        Err(error) => VerificationCheck::fail("observation_structural_privacy", error),
    });
    report
        .checks
        .push(match reproducibility_check(&config, policy, &game) {
            Ok(()) => VerificationCheck::pass("seed_reproducible"),
            Err(error) => VerificationCheck::fail("seed_reproducible", error),
        });
    report.elapsed_millis = started.elapsed().as_millis() as u64;
    report
}

fn play(config: &Config, policy: VerifiedPolicy) -> Result<Game, String> {
    let mut game = Game::new(config.clone());
    // A single policy for every seat, so the mode itself is what is measured.
    let details: &dyn DetailPolicy = &SimpleDetails;
    play_to_end(&mut game, policy.space_policy(), details)?;
    Ok(game)
}

fn completed_check(game: &Game) -> Result<(), String> {
    if !game.public.finished {
        return Err(format!("game did not finish (round {})", game.public.round));
    }
    if game.public.round != 14 {
        return Err(format!(
            "finished at round {} instead of 14",
            game.public.round
        ));
    }
    if game.log.events.is_empty() {
        return Err("finished with an empty action log".into());
    }
    Ok(())
}

fn invariants_check(game: &Game) -> Result<(), String> {
    game.validate()
}

fn replay_check(config: &Config, game: &Game) -> Result<(), String> {
    // `apply_recorded` re-validates the full state after every action, so a
    // replay error also covers the per-action invariant check.
    let replayed = Game::replay(config.clone(), &game.log.events)
        .map_err(|error| format!("replay failed: {error}"))?;
    if replayed.state_hash() != game.state_hash() {
        return Err("replayed state hash differs from the played state".into());
    }
    Ok(())
}

fn save_restore_check(game: &Game) -> Result<(), String> {
    let saved = game
        .save_json()
        .map_err(|error| format!("save_json failed: {error}"))?;
    // `load_json` re-verifies the event chain and version metadata.
    let restored = Game::load_json(&saved).map_err(|error| format!("load_json failed: {error}"))?;
    if restored.state_hash() != game.state_hash() {
        return Err("restored state hash differs from the saved state".into());
    }
    Ok(())
}

fn privacy_check(game: &Game) -> Result<(), String> {
    let observations = (0..game.public.players.len())
        .map(|viewer| game.observation(viewer))
        .collect::<Result<Vec<_>, _>>()?;
    check_observations(&game.cards.hands, &observations)
}

/// Structural privacy check, factored out so a card-mode fixture can exercise it
/// with non-empty hands. Cardless runs have empty hands, so this function alone
/// would not detect a non-empty leak; the fixture test supplies private data.
fn check_observations(hands: &[Vec<String>], observations: &[Observation]) -> Result<(), String> {
    if observations.len() != hands.len() {
        return Err("observation count differs from the number of hands".into());
    }
    let counts: Vec<usize> = hands.iter().map(Vec::len).collect();
    for (viewer, observation) in observations.iter().enumerate() {
        if observation.player_id != viewer {
            return Err(format!("observation {viewer} reports another viewer id"));
        }
        if observation.hand != hands[viewer] {
            return Err(format!("viewer {viewer} sees a hand that is not their own"));
        }
        if observation.hand_counts != counts {
            return Err(format!("viewer {viewer} sees wrong hand counts"));
        }
        let encoded =
            serde_json::to_string(observation).map_err(|error| format!("encode: {error}"))?;
        // Structural leak guard: hidden state must not be part of the JSON that
        // is handed to a policy.
        for forbidden in ["hidden_round_deck", "undealt", "\"rng\"", "\"hands\""] {
            if encoded.contains(forbidden) {
                return Err(format!(
                    "viewer {viewer} observation exposes hidden key {forbidden}"
                ));
            }
        }
        let own: std::collections::BTreeSet<&String> = observation.hand.iter().collect();
        for (other, hand) in hands.iter().enumerate() {
            if other == viewer {
                continue;
            }
            for card in hand {
                if own.contains(card) {
                    return Err(format!("viewer {viewer} sees another player's card {card}"));
                }
            }
        }
    }
    Ok(())
}

fn reproducibility_check(
    config: &Config,
    policy: VerifiedPolicy,
    game: &Game,
) -> Result<(), String> {
    let again = play(config, policy)?;
    if again.state_hash() != game.state_hash() {
        return Err("the same seed produced a different final state".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Non-vacuity: a tampered event must make the replay check fail, otherwise
    /// the check could pass on any log.
    #[test]
    fn replay_check_rejects_a_tampered_event() {
        let mode = ModeSpec::new(2, false).unwrap();
        let config = mode.config(11);
        let mut game = play(&config, VerifiedPolicy::Random).unwrap();
        replay_check(&config, &game).unwrap();
        game.log.events[0].after = "0".repeat(64);
        assert!(replay_check(&config, &game).is_err());
    }

    /// Non-vacuity: an unfinished game must not satisfy the completion check.
    #[test]
    fn completion_check_rejects_an_unfinished_game() {
        let mode = ModeSpec::new(3, true).unwrap();
        let game = Game::new(mode.config(5));
        assert!(completed_check(&game).is_err());
    }

    /// The observation handed to a policy must not serialise hidden state.
    #[test]
    fn observation_serialisation_has_no_hidden_keys() {
        let mode = ModeSpec::new(4, false).unwrap();
        let game = play(&mode.config(9), VerifiedPolicy::Random).unwrap();
        privacy_check(&game).unwrap();
        let encoded = serde_json::to_string(&game.observation(0).unwrap()).unwrap();
        assert!(!encoded.contains("hidden_round_deck"));
        assert!(!encoded.contains("undealt"));
    }

    /// P2 review item: the cardless run has empty hands, so the structural check
    /// is proven here against a card-mode fixture with non-empty private hands.
    #[test]
    fn privacy_check_rejects_a_leaked_card_in_a_card_mode_fixture() {
        let config = Config {
            seed: 42,
            players: 2,
            additional_action_space_enabled: false,
        };
        let card_set = crate::cards::verified_basic_card_set(2).unwrap();
        let game = Game::with_cards(config, card_set).unwrap();
        let hands = game.cards.hands.clone();
        assert!(
            hands.iter().all(|hand| !hand.is_empty()),
            "the fixture must carry non-empty private hands"
        );
        let observations = (0..2)
            .map(|viewer| game.observation(viewer).unwrap())
            .collect::<Vec<_>>();
        check_observations(&hands, &observations).unwrap();
        // A viewer that sees an opponent's card must be rejected.
        let mut leaked = observations.clone();
        leaked[0].hand.push(hands[1][0].clone());
        let error = check_observations(&hands, &leaked).unwrap_err();
        assert!(error.contains("not their own") || error.contains("another player"));
        // Hand counts must describe every seat, not only the viewer. Both seats
        // hold the same number of cards here, so a wrong count is what is tested.
        let mut counts_leaked = observations.clone();
        counts_leaked[0].hand_counts = vec![hands[0].len() + 1, hands[1].len()];
        assert!(check_observations(&hands, &counts_leaked).is_err());
    }

    /// P2 review item: an overflowing seed range must be refused, not wrapped.
    #[test]
    fn seed_range_rejects_overflow_and_zero() {
        assert_eq!(seed_range(700_000, 1_000).unwrap(), (700_000, 700_999));
        assert_eq!(seed_range(u64::MAX, 1).unwrap(), (u64::MAX, u64::MAX));
        assert_eq!(
            seed_range(u64::MAX - 1, 2).unwrap(),
            (u64::MAX - 1, u64::MAX)
        );
        assert!(seed_range(u64::MAX - 1, 3).is_err());
        assert!(seed_range(0, 0).is_err());
    }
}
