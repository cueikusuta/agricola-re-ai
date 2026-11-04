//! Seed-paired, seat-balanced M2 policy benchmarking.
use crate::model::{Config, Game, Observation, Space};
use crate::policy::{play_to_end, SimpleDetails, SpacePolicy};
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
    if seeds == 0 {
        return Err("benchmark needs at least one seed".into());
    }
    let mut left_wins = 0usize;
    let mut right_wins = 0usize;
    let mut ties = 0usize;
    let mut sum_left = 0i64;
    let mut sum_right = 0i64;
    for n in 0..seeds {
        for swapped in [false, true] {
            let mut game = Game::new(Config {
                seed: first_seed.wrapping_add(n as u64),
                additional_action_space_enabled,
            });
            let seats = if swapped {
                SeatPolicies {
                    seats: [right, left],
                }
            } else {
                SeatPolicies {
                    seats: [left, right],
                }
            };
            play_to_end(&mut game, &seats, &SimpleDetails)?;
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
