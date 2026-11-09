//! M8-3 (#110): evaluation-guided PUCT search for few-simulation budgets.
//!
//! The baseline `MctsPolicy` spreads a small simulation budget evenly over all
//! root moves and estimates each value with a full terminal playout. At 32
//! simulations that estimate is noisy and expensive. This module keeps the same
//! information-safety contract but changes three things:
//!
//! 1. **Priors.** The M2 evaluation function scores every legal move at a node.
//!    A softmax of those scores becomes the PUCT prior `P`, so visits flow to
//!    moves the evaluation already likes.
//! 2. **Leaf values.** `LeafMode::Heuristic` replaces the terminal playout with
//!    a depth-limited leaf evaluation, which is much cheaper and lower variance.
//!    `LeafMode::Terminal` keeps the baseline playout for comparison.
//! 3. **Root value.** Leaf evaluations are converted to a softmax first-place
//!    credit so the internal objective stays aligned with the project's
//!    primary metric (first place), not raw score.
//!
//! No opponent hand, future round card or internal seed is read: every
//! simulation samples hidden information from [`Observation`] only, through the
//! same `sample_game` helper the baseline uses.
use crate::evaluation::{action_value_with_details, evaluate_player, WeightedEvaluationPolicy};
use crate::mcts::{rollout, sample_game, MctsOutcome, SearchLimit, SearchMode};
use crate::model::{Game, Observation, Space};
use crate::policy::{weight, DetailPolicy, SimpleDetails, SpacePolicy};
use crate::score::standings_with_cards;
use rand_chacha::ChaCha8Rng;
use std::collections::BTreeMap;
use std::time::Instant;

/// How the value of a depth-limited leaf is estimated.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LeafMode {
    /// Play the game to the end with the evaluation policy (baseline style).
    Terminal,
    /// Evaluate the sampled state with the M2 heuristic (depth-limited search).
    Heuristic,
}

/// Tunable search configuration. Defaults target the few-simulation regime.
#[derive(Clone, Copy, Debug)]
pub struct PuctConfig {
    pub simulations: usize,
    pub tree_depth: usize,
    /// Exploration constant in `Q + c * P * sqrt(N + 1) / (1 + n)`.
    pub c_puct: f64,
    /// Softmax temperature applied to evaluation action values for priors.
    pub prior_temperature: f64,
    /// Softmax temperature that converts leaf evaluations into first-place
    /// credit. Smaller values make the credit sharper.
    pub value_temperature: f64,
    pub leaf_mode: LeafMode,
    /// If > 0, only this many top-prior root moves are searched.
    pub root_candidate_limit: usize,
    /// Weight of the evaluation prior in the final root choice. A positive
    /// value keeps the search anchored to the strong one-step policy and only
    /// lets the lookahead override it when the evidence is stronger.
    pub final_prior_weight: f64,
}

impl Default for PuctConfig {
    fn default() -> Self {
        // Since #263 the shipped default is the adopted configuration
        // `puct-s32-d2-c1.5-pt6-vt8-terminal-rl0-fp0` from the pre-registered
        // formal evaluation Run 42654611147 (#219): four-player cardless,
        // 32 simulations per decision, unknown seeds 985000-985249,
        // 1,000 games per arm, +8.05 pp first-credit, 95% CI [+5.2, +11.0].
        // That comparison used a fixed simulation count and says nothing about
        // equal wall-clock time; carded, draft, and two-player strength are
        // not established here. The historical configuration
        // `puct-s32-d2-c1.5-pt3-vt8-terminal-rl0-fp0` (docs/m8-3-
        // mcts-improvement-2026-11-06.md) stays reproducible by passing
        // `--puct-prior-temp 3` explicitly.
        Self {
            simulations: 32,
            tree_depth: 2,
            c_puct: 1.5,
            prior_temperature: 6.0,
            value_temperature: 8.0,
            leaf_mode: LeafMode::Terminal,
            root_candidate_limit: 0,
            final_prior_weight: 0.0,
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct PuctPolicy {
    pub config: PuctConfig,
}

#[derive(Clone, Debug)]
struct Node {
    visits: u32,
    values: Vec<f64>,
    prior: f64,
    children: BTreeMap<Space, usize>,
}

impl Node {
    fn new(players: usize, prior: f64) -> Self {
        Self {
            visits: 0,
            values: vec![0.0; players],
            prior,
            children: BTreeMap::new(),
        }
    }
}

/// Turn a vector of action values into a probability distribution.
/// Non-finite or absent scores get zero probability; if nothing is usable the
/// distribution is uniform, which degrades safely to plain UCT-like search.
fn softmax_prior(values: &[Option<f64>], temperature: f64) -> Vec<f64> {
    if values.is_empty() {
        return Vec::new();
    }
    let temperature = temperature.max(1e-6);
    let max = values
        .iter()
        .filter_map(|value| *value)
        .fold(f64::NEG_INFINITY, f64::max);
    let weights: Vec<f64> = values
        .iter()
        .map(|value| match value {
            Some(score) if score.is_finite() && max.is_finite() => {
                ((score - max) / temperature).exp()
            }
            _ => 0.0,
        })
        .collect();
    let sum: f64 = weights.iter().sum();
    if !sum.is_finite() || sum <= 0.0 {
        return vec![1.0 / weights.len() as f64; weights.len()];
    }
    weights.iter().map(|weight| weight / sum).collect()
}

/// Convert per-player heuristic evaluations into a first-place credit vector.
/// This is the search's internal objective; it is not the official standings.
fn heuristic_credit(game: &Game, temperature: f64) -> Vec<f64> {
    let players = game.public.players.len();
    let temperature = temperature.max(1e-6);
    let evaluations: Vec<f64> = game
        .public
        .players
        .iter()
        .map(|player| evaluate_player(player, game.public.round).total())
        .collect();
    let max = evaluations
        .iter()
        .copied()
        .fold(f64::NEG_INFINITY, f64::max);
    let weights: Vec<f64> = evaluations
        .iter()
        .map(|score| ((score - max) / temperature).exp())
        .collect();
    let sum: f64 = weights.iter().sum();
    if !sum.is_finite() || sum <= 0.0 {
        return vec![1.0 / players as f64; players];
    }
    weights.iter().map(|weight| weight / sum).collect()
}

/// Terminal outcome credit: `1/k` for a tie of `k` first places, else 0.
fn terminal_credit(game: &Game) -> Vec<f64> {
    let ranks = standings_with_cards(&game.public.players, &game.card_set);
    let top_count = ranks.rank.iter().filter(|&&rank| rank == 1).count() as f64;
    ranks
        .rank
        .iter()
        .map(|&rank| if rank == 1 { 1.0 / top_count } else { 0.0 })
        .collect()
}

/// Evaluation-based fallback that is always legal. Mirrors the baseline MCTS
/// fallback so a deadline never yields an illegal or panicking move.
fn fallback(obs: &Observation, legal: &[Space], rng: &mut ChaCha8Rng) -> Option<Space> {
    let details = SimpleDetails;
    WeightedEvaluationPolicy {
        prior_weight: 0.5,
        state_weight: 2.0,
    }
    .select_with_details(obs, legal, rng, &details)
}

/// The same value the M2 evaluation policy maximises: projected state change
/// plus the M1 tactical prior. Using it as the PUCT prior keeps the search
/// anchored to the baseline policy at one step while still allowing lookahead.
fn tactical_value(obs: &Observation, space: Space) -> Option<f64> {
    let delta = action_value_with_details(obs, space, &SimpleDetails)?;
    Some(2.0 * delta + 0.5 * f64::from(weight(obs, space)))
}

/// Prior probability for each move, derived from the current actor's own
/// observation only. Missing scores are handled by `softmax_prior`.
fn action_priors(obs: &Observation, moves: &[Space], temperature: f64) -> Vec<f64> {
    let values: Vec<Option<f64>> = moves
        .iter()
        .map(|space| tactical_value(obs, *space))
        .collect();
    softmax_prior(&values, temperature)
}

/// Expand `node_id`: create one child per move with its stored prior.
fn expand(nodes: &mut Vec<Node>, node_id: usize, moves: &[Space], priors: &[f64], players: usize) {
    for (index, space) in moves.iter().enumerate() {
        if nodes[node_id].children.contains_key(space) {
            continue;
        }
        let child = nodes.len();
        nodes.push(Node::new(
            players,
            priors.get(index).copied().unwrap_or(0.0),
        ));
        nodes[node_id].children.insert(*space, child);
    }
}

/// Pick the PUCT child. Unvisited children receive `c * P * sqrt(N + 1)`, so the
/// highest-prior move is tried first. Ties break deterministically by Space.
fn select_puct_child(
    nodes: &[Node],
    node_id: usize,
    moves: &[Space],
    actor: usize,
    c_puct: f64,
) -> Option<Space> {
    let parent = &nodes[node_id];
    let parent_visits = f64::from(parent.visits);
    moves
        .iter()
        .filter_map(|space| {
            let child_id = parent.children.get(space)?;
            let stats = &nodes[*child_id];
            let visits = f64::from(stats.visits);
            let mean_value = if stats.visits == 0 {
                0.0
            } else {
                stats.values[actor] / visits
            };
            let exploration = c_puct * stats.prior * (parent_visits + 1.0).sqrt() / (1.0 + visits);
            Some((*space, mean_value + exploration))
        })
        .max_by(|(space_a, score_a), (space_b, score_b)| {
            score_a
                .total_cmp(score_b)
                .then_with(|| space_b.cmp(space_a))
        })
        .map(|(space, _)| space)
}

impl PuctPolicy {
    /// PUCT selection with an optional wall-clock deadline. With
    /// `deadline = None` the result is deterministic for a fixed RNG seed.
    pub fn select_with_budget(
        &self,
        obs: &Observation,
        legal: &[Space],
        rng: &mut ChaCha8Rng,
        deadline: Option<Instant>,
    ) -> Option<(Space, MctsOutcome)> {
        let limit = match deadline {
            None => SearchLimit::Simulations(self.config.simulations),
            Some(deadline) => SearchLimit::TimeBudget {
                deadline,
                max_simulations: self.config.simulations,
            },
        };
        self.select_with_limit(obs, legal, rng, limit)
    }

    /// PUCT selection under an explicit stopping rule (#164), matching
    /// [`crate::mcts::MctsPolicy::select_with_limit`].
    ///
    /// In `TimeBudgeted` mode the loop keeps going after every completed
    /// simulation while wall-clock time remains, so the budget is actually
    /// consumed. An interrupted simulation is discarded and never applied to
    /// the tree. The returned `Space` is always a member of `legal`.
    pub fn select_with_limit(
        &self,
        obs: &Observation,
        legal: &[Space],
        rng: &mut ChaCha8Rng,
        limit: SearchLimit,
    ) -> Option<(Space, MctsOutcome)> {
        let mode = limit.mode();
        let started = Instant::now();
        let (budget, deadline, max_simulations) = match limit {
            SearchLimit::Simulations(n) => (None, None, n),
            SearchLimit::TimeBudget {
                deadline,
                max_simulations,
            } => (
                Some(deadline.saturating_duration_since(started)),
                Some(deadline),
                max_simulations,
            ),
        };
        let outcome = |simulations_completed: usize,
                       used_fallback: bool,
                       timed_out: bool,
                       elapsed: std::time::Duration| MctsOutcome {
            simulations_completed,
            used_fallback,
            timed_out,
            elapsed_ms: elapsed.as_millis().min(u128::from(u64::MAX)) as u64,
            overshoot_ms: budget
                .map(|budget| elapsed.saturating_sub(budget).as_millis() as u64)
                .unwrap_or(0),
            mode,
        };
        let immediate = outcome(0, false, false, started.elapsed());
        if legal.is_empty() {
            return None;
        }
        if legal.len() == 1 || max_simulations == 0 {
            return Some((legal[0], immediate));
        }
        // A budget that is already spent must not pay for root priors or a
        // single expansion: return the guaranteed legal fallback immediately,
        // exactly as `MctsPolicy::select_with_limit` does.
        if let Some(deadline) = deadline {
            if Instant::now() >= deadline {
                let space = fallback(obs, legal, rng)
                    .filter(|space| legal.contains(space))
                    .or_else(|| legal.first().copied())?;
                return Some((space, outcome(0, true, true, started.elapsed())));
            }
        }
        debug_assert!(matches!(
            mode,
            SearchMode::SimulationCapped | SearchMode::TimeBudgeted
        ));
        let players = obs.public.players.len();
        let details = SimpleDetails;

        // Root priors use the real observation, so they are computed once.
        let root_priors = action_priors(obs, legal, self.config.prior_temperature);
        let mut root_moves: Vec<Space> = legal.to_vec();
        if self.config.root_candidate_limit > 0
            && self.config.root_candidate_limit < root_moves.len()
        {
            let mut ranked = root_moves
                .iter()
                .copied()
                .zip(root_priors.iter().copied())
                .collect::<Vec<_>>();
            ranked.sort_by(|(space_a, prior_a), (space_b, prior_b)| {
                prior_b
                    .total_cmp(prior_a)
                    .then_with(|| space_a.cmp(space_b))
            });
            ranked.truncate(self.config.root_candidate_limit);
            root_moves = ranked.into_iter().map(|(space, _)| space).collect();
        }

        let mut nodes = vec![Node::new(players, 1.0)];
        let mut simulations_completed = 0usize;
        let mut timed_out = false;
        for _ in 0..max_simulations {
            if let Some(limit) = deadline {
                if Instant::now() >= limit {
                    timed_out = true;
                    break;
                }
            }
            let Some(mut game) = sample_game(obs, rng) else {
                continue;
            };
            let mut path = vec![0usize];
            let mut node_id = 0usize;
            let mut valid = true;
            for _ in 0..self.config.tree_depth {
                if let Some(limit) = deadline {
                    if Instant::now() >= limit {
                        timed_out = true;
                        valid = false;
                        break;
                    }
                }
                if game.public.finished {
                    break;
                }
                let actor = game.public.active_player;
                let moves = if node_id == 0 {
                    root_moves.clone()
                } else {
                    game.legal_spaces()
                };
                if moves.is_empty() {
                    valid = false;
                    break;
                }
                if nodes[node_id].children.is_empty() {
                    let observation = match game.observation(actor) {
                        Ok(observation) => observation,
                        Err(_) => {
                            valid = false;
                            break;
                        }
                    };
                    let priors = action_priors(&observation, &moves, self.config.prior_temperature);
                    expand(&mut nodes, node_id, &moves, &priors, players);
                }
                let Some(choice) =
                    select_puct_child(&nodes, node_id, &moves, actor, self.config.c_puct)
                else {
                    valid = false;
                    break;
                };
                let observation = match game.observation(actor) {
                    Ok(observation) => observation,
                    Err(_) => {
                        valid = false;
                        break;
                    }
                };
                let Some(action) = details.produce(&observation, choice) else {
                    valid = false;
                    break;
                };
                if game.apply(action).is_err() {
                    valid = false;
                    break;
                }
                let Some(&child_id) = nodes[node_id].children.get(&choice) else {
                    valid = false;
                    break;
                };
                path.push(child_id);
                node_id = child_id;
                if nodes[node_id].visits == 0 {
                    break;
                }
            }
            if timed_out {
                break;
            }
            if !valid {
                continue;
            }
            let credit = match self.config.leaf_mode {
                LeafMode::Heuristic => heuristic_credit(&game, self.config.value_temperature),
                LeafMode::Terminal => match rollout(&mut game, rng, deadline) {
                    Ok(true) => terminal_credit(&game),
                    Ok(false) => {
                        timed_out = true;
                        break;
                    }
                    Err(_) => continue,
                },
            };
            if credit.len() != players {
                continue;
            }
            simulations_completed += 1;
            for &index in &path {
                let node = &mut nodes[index];
                node.visits += 1;
                for (id, value) in credit.iter().enumerate() {
                    node.values[id] += value;
                }
            }
        }
        let chosen = root_moves
            .iter()
            .filter_map(|space| {
                let child = *nodes[0].children.get(space)?;
                let stats = &nodes[child];
                if stats.visits == 0 {
                    return None;
                }
                let evaluation = stats.values[obs.player_id] / f64::from(stats.visits)
                    + self.config.final_prior_weight * stats.prior;
                Some((*space, evaluation, stats.visits))
            })
            .max_by(
                |(space_a, value_a, visits_a), (space_b, value_b, visits_b)| {
                    value_a
                        .total_cmp(value_b)
                        .then_with(|| visits_a.cmp(visits_b))
                        .then_with(|| space_b.cmp(space_a))
                },
            )
            .map(|(space, _, _)| space);
        let used_fallback = chosen.is_none();
        let space = chosen.or_else(|| fallback(obs, legal, rng))?;
        Some((
            space,
            outcome(
                simulations_completed,
                used_fallback,
                timed_out,
                started.elapsed(),
            ),
        ))
    }
}

impl SpacePolicy for PuctPolicy {
    fn select(&self, obs: &Observation, legal: &[Space], rng: &mut ChaCha8Rng) -> Option<Space> {
        self.select_with_budget(obs, legal, rng, None)
            .map(|(space, _outcome)| space)
    }

    /// Consume the search window of a whole-decision budget (#167 A). The
    /// simulation count stays the safety cap, so a slow machine still stops.
    fn select_with_deadline(
        &self,
        obs: &Observation,
        legal: &[Space],
        rng: &mut ChaCha8Rng,
        deadline: Option<Instant>,
    ) -> Option<Space> {
        self.select_with_budget(obs, legal, rng, deadline)
            .map(|(space, _outcome)| space)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Config;
    use rand::SeedableRng;

    fn four_player_game() -> Game {
        Game::new(Config {
            players: 4,
            seed: 4242,
            additional_action_space_enabled: false,
        })
    }

    /// #263: the shipped default is the adopted configuration from the
    /// pre-registered formal evaluation Run 42654611147 (#219). Every field is
    /// enumerated so a future accidental change fails here instead of silently
    /// shifting the standard PUCT.
    #[test]
    fn shipped_default_is_the_adopted_pt6_configuration() {
        let config = PuctConfig::default();
        assert_eq!(config.simulations, 32);
        assert_eq!(config.tree_depth, 2);
        assert_eq!(config.c_puct, 1.5);
        assert_eq!(config.prior_temperature, 6.0);
        assert_eq!(config.value_temperature, 8.0);
        assert_eq!(config.leaf_mode, LeafMode::Terminal);
        assert_eq!(config.root_candidate_limit, 0);
        assert_eq!(config.final_prior_weight, 0.0);
    }

    #[test]
    fn softmax_prior_is_a_distribution() {
        let prior = softmax_prior(&[Some(1.0), Some(2.0), Some(3.0)], 1.0);
        assert_eq!(prior.len(), 3);
        let sum: f64 = prior.iter().sum();
        assert!((sum - 1.0).abs() < 1e-9);
        assert!(prior[2] > prior[1] && prior[1] > prior[0]);
    }

    #[test]
    fn softmax_prior_falls_back_to_uniform() {
        assert_eq!(softmax_prior(&[None, None], 1.0), vec![0.5, 0.5]);
        assert!(softmax_prior(&[], 1.0).is_empty());
    }

    #[test]
    fn heuristic_credit_sums_to_one_and_favours_the_leader() {
        let mut game = four_player_game();
        game.public.players[2].supply.food += 50;
        let credit = heuristic_credit(&game, 8.0);
        let sum: f64 = credit.iter().sum();
        assert!((sum - 1.0).abs() < 1e-9);
        assert!(credit[2] > credit[0]);
    }

    #[test]
    fn terminal_credit_sums_to_one() {
        let game = four_player_game();
        let credit = terminal_credit(&game);
        let sum: f64 = credit.iter().sum();
        assert!((sum - 1.0).abs() < 1e-9);
    }

    #[test]
    fn returns_a_legal_and_reproducible_move() {
        let game = four_player_game();
        let obs = game.observation(game.public.active_player).unwrap();
        let legal = game.legal_spaces();
        let policy = PuctPolicy {
            config: PuctConfig {
                simulations: 8,
                ..PuctConfig::default()
            },
        };
        let mut a = ChaCha8Rng::seed_from_u64(17);
        let mut b = ChaCha8Rng::seed_from_u64(17);
        let first = policy.select(&obs, &legal, &mut a);
        assert_eq!(first, policy.select(&obs, &legal, &mut b));
        assert!(legal.contains(&first.unwrap()));
    }

    #[test]
    fn heuristic_and_terminal_modes_both_stay_legal() {
        let game = four_player_game();
        let obs = game.observation(game.public.active_player).unwrap();
        let legal = game.legal_spaces();
        for leaf_mode in [LeafMode::Heuristic, LeafMode::Terminal] {
            let policy = PuctPolicy {
                config: PuctConfig {
                    simulations: 6,
                    tree_depth: 2,
                    leaf_mode,
                    ..PuctConfig::default()
                },
            };
            let mut rng = ChaCha8Rng::seed_from_u64(3);
            let space = policy.select(&obs, &legal, &mut rng).unwrap();
            assert!(legal.contains(&space));
        }
    }

    #[test]
    fn root_candidate_limit_keeps_the_choice_legal() {
        let game = four_player_game();
        let obs = game.observation(game.public.active_player).unwrap();
        let legal = game.legal_spaces();
        let policy = PuctPolicy {
            config: PuctConfig {
                simulations: 8,
                root_candidate_limit: 3,
                ..PuctConfig::default()
            },
        };
        let mut rng = ChaCha8Rng::seed_from_u64(9);
        let space = policy.select(&obs, &legal, &mut rng).unwrap();
        assert!(legal.contains(&space));
    }

    #[test]
    fn elapsed_deadline_falls_back_to_a_legal_move() {
        let game = four_player_game();
        let obs = game.observation(game.public.active_player).unwrap();
        let legal = game.legal_spaces();
        let policy = PuctPolicy {
            config: PuctConfig {
                simulations: 10_000,
                ..PuctConfig::default()
            },
        };
        let mut rng = ChaCha8Rng::seed_from_u64(4);
        let (space, outcome) = policy
            .select_with_budget(&obs, &legal, &mut rng, Some(Instant::now()))
            .unwrap();
        assert!(legal.contains(&space));
        assert!(outcome.timed_out);
        assert_eq!(outcome.simulations_completed, 0);
        assert!(outcome.used_fallback);
    }

    /// Regression for the review finding on PR #166: an expired budget must be
    /// detected *before* root priors and expansions, so the decision reports a
    /// measured elapsed time near zero instead of paying for a full prior pass.
    #[test]
    fn an_already_spent_budget_returns_before_any_prior_work() {
        let game = four_player_game();
        let obs = game.observation(game.public.active_player).unwrap();
        let legal = game.legal_spaces();
        assert!(legal.len() > 1);
        let policy = PuctPolicy {
            config: PuctConfig {
                simulations: 100_000,
                ..PuctConfig::default()
            },
        };
        let mut rng = ChaCha8Rng::seed_from_u64(4);
        let (space, outcome) = policy
            .select_with_limit(
                &obs,
                &legal,
                &mut rng,
                SearchLimit::TimeBudget {
                    deadline: Instant::now(),
                    max_simulations: 100_000,
                },
            )
            .unwrap();
        assert!(legal.contains(&space), "fallback must stay legal");
        assert_eq!(outcome.mode, SearchMode::TimeBudgeted);
        assert!(outcome.timed_out);
        assert!(outcome.used_fallback);
        assert_eq!(outcome.simulations_completed, 0);
        assert!(
            outcome.elapsed_ms < 100,
            "an expired budget must not pay for root priors, took {}ms",
            outcome.elapsed_ms
        );
        assert_eq!(outcome.overshoot_ms, 0);
    }

    /// Fixed-seed reproducibility of the capped mode must stay intact after the
    /// search-limit refactor.
    #[test]
    fn simulation_capped_puct_is_seed_reproducible() {
        let game = four_player_game();
        let obs = game.observation(game.public.active_player).unwrap();
        let legal = game.legal_spaces();
        let policy = PuctPolicy {
            config: PuctConfig {
                simulations: 4,
                ..PuctConfig::default()
            },
        };
        let run = || {
            let mut rng = ChaCha8Rng::seed_from_u64(17);
            policy
                .select_with_limit(&obs, &legal, &mut rng, SearchLimit::Simulations(4))
                .unwrap()
        };
        let (first_space, first) = run();
        let (second_space, second) = run();
        assert_eq!(first_space, second_space, "fixed seed must be reproducible");
        assert_eq!(first.simulations_completed, second.simulations_completed);
        assert!(first.simulations_completed <= 4);
        assert_eq!(first.mode, SearchMode::SimulationCapped);
        assert!(!first.timed_out);
        assert_eq!(first.overshoot_ms, 0);
    }
}
