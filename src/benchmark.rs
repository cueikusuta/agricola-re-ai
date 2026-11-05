//! Seed-paired, seat-balanced M2 policy benchmarking.
use crate::cards::CardSet;
use crate::model::{Config, Game, Observation, Space};
use crate::policy::{play_to_end, CardChoiceDetails, SimpleDetails, SpacePolicy};
use crate::score::standings;
use rand_chacha::ChaCha8Rng;
use serde::Serialize;

pub struct SeatPolicies<'a> {
    pub seats: [&'a dyn SpacePolicy; 2],
}

impl SpacePolicy for SeatPolicies<'_> {
    fn select(&self, obs: &Observation, legal: &[Space], rng: &mut ChaCha8Rng) -> Option<Space> {
        self.seats.get(obs.player_id)?.select(obs, legal, rng)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct MatchSummary {
    pub seeds: usize,
    pub games: usize,
    pub additional_action_space_enabled: bool,
    pub left_wins: usize,
    pub right_wins: usize,
    pub ties: usize,
    pub left_mean_score: f64,
    pub right_mean_score: f64,
    pub left_mean_margin: f64,
    pub left_win_rate: f64,
}

/// Each seed is played twice, with both policy seat assignments.
/// A tie uses the engine's resource tiebreaker; equal ranks count as a draw.
/// The public Observation is the only input forwarded to either AI.
pub fn compare_policies(
    left: &dyn SpacePolicy,
    right: &dyn SpacePolicy,
    seeds: usize,
    first_seed: u64,
    additional_action_space_enabled: bool,
) -> Result<MatchSummary, String> {
    compare_policies_in_mode(
        left,
        right,
        seeds,
        first_seed,
        additional_action_space_enabled,
        None,
    )
}

/// Same paired seats and fixed seeds as M2, with the supplied implemented
/// cards. Both players are controlled using Observation, not private Game.
pub fn compare_policies_with_cards(
    left: &dyn SpacePolicy,
    right: &dyn SpacePolicy,
    seeds: usize,
    first_seed: u64,
    additional_action_space_enabled: bool,
    card_set: &CardSet,
) -> Result<MatchSummary, String> {
    compare_policies_in_mode(
        left,
        right,
        seeds,
        first_seed,
        additional_action_space_enabled,
        Some(card_set),
    )
}

fn compare_policies_in_mode(
    left: &dyn SpacePolicy,
    right: &dyn SpacePolicy,
    seeds: usize,
    first_seed: u64,
    additional_action_space_enabled: bool,
    card_set: Option<&CardSet>,
) -> Result<MatchSummary, String> {
    if seeds == 0 {
        return Err("benchmark needs at least one seed".into());
    }
    if let Some(set) = card_set {
        set.validate(2)?;
    }
    let mut left_wins = 0usize;
    let mut right_wins = 0usize;
    let mut ties = 0usize;
    let mut sum_left = 0i64;
    let mut sum_right = 0i64;
    for n in 0..seeds {
        for swapped in [false, true] {
            let config = Config {
                players: 2,
                seed: first_seed.wrapping_add(n as u64),
                additional_action_space_enabled,
            };
            let mut game = if let Some(set) = card_set {
                Game::with_cards(config, set.clone())?
            } else {
                Game::new(config)
            };
            let seats = if swapped {
                SeatPolicies {
                    seats: [right, left],
                }
            } else {
                SeatPolicies {
                    seats: [left, right],
                }
            };
            if card_set.is_some() {
                play_to_end(&mut game, &seats, &CardChoiceDetails)?;
            } else {
                play_to_end(&mut game, &seats, &SimpleDetails)?;
            }
            let result = standings(&game.public.players);
            let left_id = usize::from(swapped);
            let right_id = 1 - left_id;
            sum_left += i64::from(result.scores[left_id].total);
            sum_right += i64::from(result.scores[right_id].total);
            match result.rank[left_id].cmp(&result.rank[right_id]) {
                std::cmp::Ordering::Less => left_wins += 1,
                std::cmp::Ordering::Greater => right_wins += 1,
                std::cmp::Ordering::Equal => ties += 1,
            }
        }
    }
    let games = 2 * seeds;
    let denominator = games as f64;
    Ok(MatchSummary {
        seeds,
        games,
        additional_action_space_enabled,
        left_wins,
        right_wins,
        ties,
        left_mean_score: sum_left as f64 / denominator,
        right_mean_score: sum_right as f64 / denominator,
        left_mean_margin: (sum_left - sum_right) as f64 / denominator,
        left_win_rate: (left_wins as f64 + ties as f64 * 0.5) / denominator,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::verified_basic_card_set;
    use crate::evaluation::EvaluationPolicy;
    use crate::policy::GreedyPolicy;

    #[test]
    fn card_matches_are_seat_balanced_seeded_and_keep_margin_separate() {
        let set = verified_basic_card_set(2).unwrap();
        let run = || {
            compare_policies_with_cards(&EvaluationPolicy, &GreedyPolicy, 1, 901, false, &set)
                .unwrap()
        };
        let first = run();
        assert_eq!(first, run());
        assert_eq!(first.games, 2);
        assert_eq!(first.left_wins + first.right_wins + first.ties, 2);
        assert_eq!(
            first.left_mean_margin,
            first.left_mean_score - first.right_mean_score
        );
    }

    #[test]
    fn invalid_or_empty_benchmark_is_rejected() {
        let set = verified_basic_card_set(2).unwrap();
        assert!(
            compare_policies_with_cards(&EvaluationPolicy, &GreedyPolicy, 0, 0, false, &set,)
                .is_err()
        );
    }
}
