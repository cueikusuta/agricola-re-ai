//! M3: information-safe, re-determinized Monte Carlo tree search.
//! A fresh hidden round order is sampled from the public revealed cards for
//! each simulation. No Game reference (or hidden RNG) enters SpacePolicy.
use crate::evaluation::EvaluationPolicy;
use crate::model::{Config, Game, Observation, RoundCard, Space};
use crate::policy::{DetailPolicy, GreedyPolicy, SimpleDetails, SpacePolicy};
use crate::score::standings;
use rand::{seq::SliceRandom, Rng};
use rand_chacha::ChaCha8Rng;
use std::collections::BTreeMap;

/// Win probability comes first; only empirically equivalent win rates
/// (absolute difference <= 2.5 percentage points) use score margin.
pub const WIN_RATE_TOLERANCE: f64 = 0.025;

#[derive(Clone, Copy, Debug)]
pub struct MctsPolicy {
    pub simulations: usize,
    pub tree_depth: usize,
}

impl Default for MctsPolicy {
    fn default() -> Self {
        Self {
            simulations: 48,
            tree_depth: 3,
        }
    }
}

#[derive(Clone, Debug)]
struct Node {
    visits: u32,
    wins: Vec<f64>,
    margins: Vec<f64>,
    children: BTreeMap<Space, usize>,
}

impl Node {
    fn new(players: usize) -> Self {
        Self {
            visits: 0,
            wins: vec![0.0; players],
            margins: vec![0.0; players],
            children: BTreeMap::new(),
        }
    }
}

/// Creates a plausible full state from *only* public information.
/// The remaining round cards are sampled independently within each stage.
/// This is a rollout approximation: past Well purchase times are not part
/// of Observation, so pending Well rewards cannot be reconstructed.
fn sample_game(obs: &Observation, rng: &mut ChaCha8Rng) -> Game {
    let seed: u64 = rng.gen();
    let mut game = Game::new(Config {
        seed,
        additional_action_space_enabled: obs.public.additional_action_space_enabled,
    });
    game.public = obs.public.clone();
    let mut deck = Vec::new();
    for stage in 1..=6 {
        let mut remaining = RoundCard::ALL
            .iter()
            .copied()
            .filter(|card| card.stage() == stage && !obs.public.revealed.contains(card))
            .collect::<Vec<_>>();
        remaining.shuffle(rng);
        deck.extend(remaining);
    }
    deck.reverse(); // Game::start_round pops the next card from the end.
    game.hidden_round_deck = deck;
    game
}

/// Compare every root action to the same best observed win rate.
/// A pairwise tolerance comparator is non-transitive and makes the result
/// dependent on the order of legal actions.
fn best_root_action(candidates: &[(Space, (f64, f64))]) -> Option<Space> {
    let highest_win = candidates
        .iter()
        .map(|(_, value)| value.0)
        .max_by(f64::total_cmp)?;
    candidates
        .iter()
        .filter(|(_, value)| highest_win - value.0 <= WIN_RATE_TOLERANCE)
        .max_by(|(space_a, a), (space_b, b)| {
            a.1.total_cmp(&b.1)
                .then_with(|| a.0.total_cmp(&b.0))
                .then_with(|| space_b.cmp(space_a))
        })
        .map(|(space, _)| *space)
}

fn rollout(game: &mut Game, rng: &mut ChaCha8Rng) -> Result<(), String> {
    let details = SimpleDetails;
    let evaluation = EvaluationPolicy;
    let greedy = GreedyPolicy;
    for _ in 0..250 {
        if game.public.finished {
            return Ok(());
        }
        let id = game.public.active_player;
        let obs = game.observation(id)?;
        let legal = game.legal_spaces();
        if legal.is_empty() {
            return Err("rollout has no legal move".into());
        }
        // Evaluation-guided playout, with a cheaper tactical fallback.
        let policy: &dyn SpacePolicy = if rng.gen_range(0..4) == 0 {
            &greedy
        } else {
            &evaluation
        };
        let space = policy
            .select(&obs, &legal, rng)
            .ok_or("rollout policy failed")?;
        let action = details
            .produce(&obs, space)
            .ok_or("rollout detail failed")?;
        game.apply(action)?;
    }
    Err("rollout move limit reached".into())
}

impl SpacePolicy for MctsPolicy {
    fn select(&self, obs: &Observation, legal: &[Space], rng: &mut ChaCha8Rng) -> Option<Space> {
        if legal.is_empty() {
            return None;
        }
        if legal.len() == 1 || self.simulations == 0 {
            return Some(legal[0]);
        }
        let player_count = obs.public.players.len();
        let mut nodes = vec![Node::new(player_count)];
        let detail_policy = SimpleDetails;
        for _ in 0..self.simulations {
            let mut game = sample_game(obs, rng);
            let mut path = vec![0usize];
            let mut node_id = 0usize;
            let mut valid = true;
            for _ in 0..self.tree_depth {
                if game.public.finished {
                    break;
                }
                let actor = game.public.active_player;
                let moves = game.legal_spaces();
                if moves.is_empty() {
                    valid = false;
                    break;
                }
                let unexpanded: Vec<_> = moves
                    .iter()
                    .copied()
                    .filter(|s| !nodes[node_id].children.contains_key(s))
                    .collect();
                let choice = if let Some(&s) = unexpanded.choose(rng) {
                    s
                } else {
                    let total = (nodes[node_id].visits.max(1) as f64).ln();
                    let candidates = moves.iter().filter_map(|space| {
                        let child = nodes[node_id].children.get(space)?;
                        let stats = &nodes[*child];
                        let n = stats.visits.max(1) as f64;
                        let win = stats.wins[actor] / n;
                        let exploration = 1.414 * (total / n).sqrt();
                        Some((*space, win + exploration))
                    });
                    candidates
                        .max_by(|a, b| a.1.total_cmp(&b.1))
                        .map(|(space, _)| space)
                        .unwrap_or(moves[0])
                };
                let observation = match game.observation(actor) {
                    Ok(o) => o,
                    Err(_) => {
                        valid = false;
                        break;
                    }
                };
                let Some(action) = detail_policy.produce(&observation, choice) else {
                    valid = false;
                    break;
                };
                if game.apply(action).is_err() {
                    valid = false;
                    break;
                }
                let (next, created) = if let Some(&index) = nodes[node_id].children.get(&choice) {
                    (index, false)
                } else {
                    let index = nodes.len();
                    nodes.push(Node::new(player_count));
                    nodes[node_id].children.insert(choice, index);
                    (index, true)
                };
                path.push(next);
                node_id = next;
                if created {
                    break;
                }
            }
            if !valid || rollout(&mut game, rng).is_err() {
                continue;
            }
            let ranks = standings(&game.public.players);
            let top_count = ranks.rank.iter().filter(|&&rank| rank == 1).count() as f64;
            for &index in &path {
                let node = &mut nodes[index];
                node.visits += 1;
                for id in 0..player_count {
                    if ranks.rank[id] == 1 {
                        node.wins[id] += 1.0 / top_count;
                    }
                    let rival = ranks
                        .scores
                        .iter()
                        .enumerate()
                        .filter(|(rival_id, _)| *rival_id != id)
                        .map(|(_, s)| s.total)
                        .max()
                        .unwrap_or(ranks.scores[id].total);
                    node.margins[id] += f64::from(ranks.scores[id].total - rival);
                }
            }
        }
        let mut candidates = Vec::new();
        for &space in legal {
            let Some(&child) = nodes[0].children.get(&space) else {
                continue;
            };
            let n = nodes[child].visits;
            if n == 0 {
                continue;
            }
            let value = (
                nodes[child].wins[obs.player_id] / f64::from(n),
                nodes[child].margins[obs.player_id] / f64::from(n),
            );
            candidates.push((space, value));
        }
        best_root_action(&candidates).or_else(|| EvaluationPolicy.select(obs, legal, rng))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::SeedableRng;

    #[test]
    fn sampler_obeys_public_state_and_stage_boundaries() {
        let mut game = Game::new(Config::default());
        let obs = game.observation(game.public.active_player).unwrap();
        let mut rng = ChaCha8Rng::seed_from_u64(11);
        for _ in 0..20 {
            let sampled = sample_game(&obs, &mut rng);
            assert_eq!(sampled.public, obs.public);
            sampled.validate().unwrap();
            assert_eq!(
                sampled.hidden_round_deck.len(),
                14 - obs.public.revealed.len()
            );
        }
        game.public.revealed.clear(); // changing public info changes what is sampled
        let original = sample_game(&obs, &mut rng);
        assert_eq!(original.public.revealed.len(), 1);
    }

    #[test]
    fn score_margin_only_breaks_nearly_equal_win_rates() {
        let clear_winner = [
            (Space::Forest, (0.6, -10.0)),
            (Space::ClayPit, (0.5, 100.0)),
        ];
        assert_eq!(best_root_action(&clear_winner), Some(Space::Forest));
        let close = [(Space::Forest, (0.50, 4.0)), (Space::ClayPit, (0.51, 1.0))];
        assert_eq!(best_root_action(&close), Some(Space::Forest));
    }

    #[test]
    fn win_rate_tolerance_is_order_independent_and_anchored_to_best() {
        let scores = [
            (Space::Forest, (0.56, 1000.0)),
            (Space::ClayPit, (0.58, 50.0)),
            (Space::ReedBank, (0.60, -1.0)),
        ];
        assert_eq!(best_root_action(&scores), Some(Space::ClayPit));
        let reversed = scores.iter().copied().rev().collect::<Vec<_>>();
        assert_eq!(best_root_action(&reversed), Some(Space::ClayPit));
        assert_eq!(best_root_action(&[]), None);
    }

    #[test]
    fn returns_legal_and_seed_reproducible_choice() {
        for enabled in [false, true] {
            let game = Game::new(Config {
                seed: 37,
                additional_action_space_enabled: enabled,
            });
            let legal = game.legal_spaces();
            let obs = game.observation(game.public.active_player).unwrap();
            let policy = MctsPolicy {
                simulations: 8,
                tree_depth: 2,
            };
            let mut a = ChaCha8Rng::seed_from_u64(23);
            let mut b = ChaCha8Rng::seed_from_u64(23);
            let one = policy.select(&obs, &legal, &mut a);
            let two = policy.select(&obs, &legal, &mut b);
            assert_eq!(one, two);
            assert!(legal.contains(&one.unwrap()));
        }
    }
}
