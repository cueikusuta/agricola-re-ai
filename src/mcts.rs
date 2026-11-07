//! M3: information-safe, re-determinized Monte Carlo tree search.
//! A fresh hidden round order is sampled from the public revealed cards for
//! each simulation. No Game reference (or hidden RNG) enters SpacePolicy.
use crate::cards::{CardKind, CardSet, CardState};
use crate::evaluation::{DetailAwareEvaluationPolicy, WeightedEvaluationPolicy};
use crate::model::{Config, Game, Observation, RoundCard, Space};
use crate::policy::{CardChoiceDetails, DetailPolicy, GreedyPolicy, SpacePolicy};
use crate::score::standings_with_cards;
use rand::{seq::SliceRandom, Rng};
use rand_chacha::ChaCha8Rng;
use std::collections::{BTreeMap, BTreeSet};
use std::time::Instant;

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
/// Rebuild the hidden card partition from the observing player's information.
/// Unknown cards are shuffled separately by kind; opponent cards are never copied
/// from the real Game, even when the caller has a reference to that Game.
fn sample_private_cards(obs: &Observation, rng: &mut ChaCha8Rng) -> Option<(CardSet, CardState)> {
    let players = obs.public.players.len();
    if obs.player_id >= players || obs.hand_counts.len() != players {
        return None;
    }
    let set = CardSet {
        definitions: obs.card_definitions.clone(),
    };
    set.validate(players).ok()?;
    if !set.enabled() {
        if obs.hand_counts.iter().any(|&n| n != 0) || !obs.hand.is_empty() {
            return None;
        }
        return Some((
            set,
            CardState {
                hands: vec![Vec::new(); players],
                undealt: Vec::new(),
            },
        ));
    }

    // All public played cards and only the observer's own hand are known.
    let mut known = BTreeSet::new();
    for id in obs
        .public
        .players
        .iter()
        .flat_map(|p| p.played_cards.iter())
        .chain(obs.hand.iter())
    {
        if set.definition(id).is_none() || !known.insert(id.clone()) {
            return None;
        }
    }
    let mut hands = vec![Vec::new(); players];
    hands[obs.player_id] = obs.hand.clone();
    let mut undealt = Vec::new();
    for kind in [CardKind::Occupation, CardKind::MinorImprovement] {
        let mut pool = set
            .definitions
            .iter()
            .filter(|c| c.kind == kind && !known.contains(&c.id))
            .map(|c| c.id.clone())
            .collect::<Vec<_>>();
        pool.shuffle(rng);
        for (id, player) in obs.public.players.iter().enumerate() {
            if id == obs.player_id {
                continue;
            }
            let played = player
                .played_cards
                .iter()
                .filter(|id| set.definition(id).is_some_and(|c| c.kind == kind))
                .count();
            let required = 7usize.checked_sub(played)?;
            if pool.len() < required {
                return None;
            }
            for _ in 0..required {
                hands[id].push(pool.pop()?);
            }
        }
        undealt.extend(pool);
    }
    if hands.iter().map(Vec::len).collect::<Vec<_>>() != obs.hand_counts {
        return None;
    }
    let cards = CardState { hands, undealt };
    cards.validate(&set, &obs.public.players).ok()?;
    Some((set, cards))
}

fn sample_game(obs: &Observation, rng: &mut ChaCha8Rng) -> Option<Game> {
    let seed: u64 = rng.gen();
    let (set, cards) = sample_private_cards(obs, rng)?;
    let mut game = Game::with_cards(
        Config {
            seed,
            additional_action_space_enabled: obs.public.additional_action_space_enabled,
            players: obs.public.players.len(),
        },
        set,
    )
    .ok()?;
    game.public = obs.public.clone();
    game.cards = cards;
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
    // Search positions are hypothetical: skip state hashing and event logging.
    game.record_events = false;
    game.validate().ok()?;
    Some(game)
}

/// The guaranteed-legal action used when the search produced nothing usable.
/// It never reads hidden state: only the observation and the legal list.
fn fallback_space(obs: &Observation, legal: &[Space], rng: &mut ChaCha8Rng) -> Option<Space> {
    let details = CardChoiceDetails;
    DetailAwareEvaluationPolicy {
        weights: WeightedEvaluationPolicy {
            prior_weight: 0.5,
            state_weight: 2.0,
        },
        details: &details,
    }
    .select(obs, legal, rng)
    .filter(|space| legal.contains(space))
    .or_else(|| legal.first().copied())
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

fn rollout(
    game: &mut Game,
    rng: &mut ChaCha8Rng,
    deadline: Option<Instant>,
) -> Result<bool, String> {
    let details = CardChoiceDetails;
    let evaluation = DetailAwareEvaluationPolicy {
        weights: WeightedEvaluationPolicy {
            prior_weight: 0.5,
            state_weight: 2.0,
        },
        details: &details,
    };
    let greedy = GreedyPolicy;
    for _ in 0..320 {
        // A single playout can be long; stop mid-playout too so the whole
        // decision budget is not overshot by one full rollout.
        if let Some(limit) = deadline {
            if Instant::now() >= limit {
                return Ok(false);
            }
        }
        if game.public.finished {
            return Ok(true);
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

/// How a search is allowed to stop (#164).
///
/// These two modes answer different questions and their results must never be
/// pooled: `SimulationCapped` fixes the amount of search (reproducible), while
/// `TimeBudgeted` fixes the wall-clock cost (not reproducible).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum SearchMode {
    /// Run exactly N completed simulations. No wall-clock limit is applied, so
    /// a fixed RNG seed reproduces the same choice, score and replay hash.
    #[default]
    SimulationCapped,
    /// Keep searching while the wall-clock deadline has not passed, up to a
    /// safety cap. The simulation count therefore depends on the machine.
    TimeBudgeted,
}

impl SearchMode {
    pub fn as_str(self) -> &'static str {
        match self {
            SearchMode::SimulationCapped => "simulation-capped",
            SearchMode::TimeBudgeted => "time-budgeted",
        }
    }
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "simulation-capped" | "simulations" => Some(SearchMode::SimulationCapped),
            "time-budgeted" | "time" => Some(SearchMode::TimeBudgeted),
            _ => None,
        }
    }
}

/// The stopping rule handed to [`MctsPolicy::select_with_limit`].
#[derive(Clone, Copy, Debug)]
pub enum SearchLimit {
    /// Stop after exactly this many completed simulations.
    Simulations(usize),
    /// Stop at `deadline`, and never run more than `max_simulations`.
    TimeBudget {
        deadline: Instant,
        max_simulations: usize,
    },
}

impl SearchLimit {
    pub fn mode(&self) -> SearchMode {
        match self {
            SearchLimit::Simulations(_) => SearchMode::SimulationCapped,
            SearchLimit::TimeBudget { .. } => SearchMode::TimeBudgeted,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MctsOutcome {
    pub simulations_completed: usize,
    pub used_fallback: bool,
    pub timed_out: bool,
    /// Wall-clock time this decision consumed inside the search.
    pub elapsed_ms: u64,
    /// How far past its deadline the decision ran. Always `0` when no deadline
    /// was in force. A non-zero value is a measurement, not a failure: the
    /// deadline can only be observed between simulations, so one simulation of
    /// overshoot is possible.
    pub overshoot_ms: u64,
    pub mode: SearchMode,
}

impl MctsPolicy {
    /// MCTS with an optional wall-clock deadline. When the deadline elapses it
    /// returns the best completed candidate, or a guaranteed legal fallback
    /// from the evaluation policy, never an illegal or panicking move. With
    /// `deadline = None` the result is deterministic for a fixed RNG seed.
    ///
    /// `Some(deadline)` keeps the historical meaning: the simulation count in
    /// `self.simulations` is a cap and the deadline may cut the search short.
    /// Prefer [`Self::select_with_limit`] when the mode must be explicit.
    pub fn select_with_budget(
        &self,
        obs: &Observation,
        legal: &[Space],
        rng: &mut ChaCha8Rng,
        deadline: Option<Instant>,
    ) -> Option<(Space, MctsOutcome)> {
        let limit = match deadline {
            None => SearchLimit::Simulations(self.simulations),
            Some(deadline) => SearchLimit::TimeBudget {
                deadline,
                max_simulations: self.simulations,
            },
        };
        self.select_with_limit(obs, legal, rng, limit)
    }

    /// Search under an explicit stopping rule (#164).
    ///
    /// In `TimeBudgeted` mode the loop keeps going after every completed
    /// simulation while time remains, so the budget is actually consumed. A
    /// simulation that is interrupted by the deadline is discarded: it is never
    /// counted and never updates the tree. The returned `Space` is always a
    /// member of `legal`.
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
        // A time budget that is already spent returns a legal fallback without
        // touching the tree.
        if let Some(deadline) = deadline {
            if Instant::now() >= deadline {
                let fallback = fallback_space(obs, legal, rng)?;
                return Some((fallback, outcome(0, true, true, started.elapsed())));
            }
        }
        let player_count = obs.public.players.len();
        let mut nodes = vec![Node::new(player_count)];
        let detail_policy = CardChoiceDetails;
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
            for _ in 0..self.tree_depth {
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
            if timed_out {
                break;
            }
            if !valid {
                continue;
            }
            match rollout(&mut game, rng, deadline) {
                Ok(true) => {}
                Ok(false) => {
                    timed_out = true;
                    break;
                }
                Err(_) => continue,
            }
            simulations_completed += 1;
            let ranks = standings_with_cards(&game.public.players, &game.card_set);
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
        let chosen = best_root_action(&candidates);
        let used_fallback = chosen.is_none();
        let space = chosen.or_else(|| fallback_space(obs, legal, rng))?;
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

impl SpacePolicy for MctsPolicy {
    fn select(&self, obs: &Observation, legal: &[Space], rng: &mut ChaCha8Rng) -> Option<Space> {
        self.select_with_budget(obs, legal, rng, None)
            .map(|(space, _outcome)| space)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::SeedableRng;

    #[test]
    fn search_mode_round_trips_and_rejects_unknown_names() {
        for mode in [SearchMode::SimulationCapped, SearchMode::TimeBudgeted] {
            assert_eq!(SearchMode::parse(mode.as_str()), Some(mode));
        }
        assert_eq!(
            SearchMode::parse("simulations"),
            Some(SearchMode::SimulationCapped)
        );
        assert_eq!(SearchMode::parse("time"), Some(SearchMode::TimeBudgeted));
        assert_eq!(SearchMode::parse("adaptive"), None);
        assert_eq!(SearchMode::default(), SearchMode::SimulationCapped);
    }

    #[test]
    fn simulation_capped_searches_a_fixed_amount_and_is_reproducible() {
        let policy = MctsPolicy {
            simulations: 6,
            tree_depth: 2,
        };
        let run = |rng_seed: u64| {
            let game = Game::new(Config {
                players: 2,
                seed: 5,
                additional_action_space_enabled: false,
            });
            let obs = game.observation(game.public.active_player).unwrap();
            let legal = game.legal_spaces();
            let mut rng = ChaCha8Rng::seed_from_u64(rng_seed);
            let (space, outcome) = policy
                .select_with_limit(&obs, &legal, &mut rng, SearchLimit::Simulations(6))
                .unwrap();
            assert!(legal.contains(&space), "the choice must stay legal");
            (space, outcome)
        };
        let (first_space, first) = run(9);
        let (second_space, second) = run(9);
        assert_eq!(first_space, second_space, "fixed seed must be reproducible");
        assert_eq!(first.simulations_completed, second.simulations_completed);
        assert!(first.simulations_completed <= 6);
        assert_eq!(first.mode, SearchMode::SimulationCapped);
        assert!(!first.timed_out);
        assert_eq!(first.overshoot_ms, 0, "no deadline is applied in this mode");
    }

    #[test]
    fn time_budgeted_search_consumes_the_budget_instead_of_stopping_at_the_cap() {
        let game = Game::new(Config {
            players: 4,
            seed: 21,
            additional_action_space_enabled: false,
        });
        let obs = game.observation(game.public.active_player).unwrap();
        let legal = game.legal_spaces();
        assert!(legal.len() > 1);
        // `simulations` is only a safety cap in time-budgeted mode. Setting it
        // to 1 proves the budget, not the cap, decides when to stop.
        let policy = MctsPolicy {
            simulations: 1,
            tree_depth: 2,
        };
        let budget = std::time::Duration::from_millis(250);
        let mut rng = ChaCha8Rng::seed_from_u64(7);
        let (space, outcome) = policy
            .select_with_limit(
                &obs,
                &legal,
                &mut rng,
                SearchLimit::TimeBudget {
                    deadline: Instant::now() + budget,
                    max_simulations: 100_000,
                },
            )
            .unwrap();
        assert!(legal.contains(&space), "the choice must stay legal");
        assert_eq!(outcome.mode, SearchMode::TimeBudgeted);
        assert!(
            outcome.simulations_completed > 0,
            "the budget was not used for any simulation"
        );
        assert!(
            outcome.timed_out,
            "a budgeted search must report that the deadline ended it"
        );
        assert!(
            outcome.elapsed_ms >= 200,
            "the deadline should end the search near the budget, got {}ms",
            outcome.elapsed_ms
        );
        assert!(
            outcome.overshoot_ms <= 1_000,
            "overshoot must stay within roughly one simulation, got {}ms",
            outcome.overshoot_ms
        );
    }

    #[test]
    fn a_longer_budget_completes_at_least_as_many_simulations() {
        let game = Game::new(Config {
            players: 2,
            seed: 33,
            additional_action_space_enabled: false,
        });
        let obs = game.observation(game.public.active_player).unwrap();
        let legal = game.legal_spaces();
        let policy = MctsPolicy {
            simulations: 1,
            tree_depth: 2,
        };
        let measure = |millis: u64| {
            let mut rng = ChaCha8Rng::seed_from_u64(11);
            policy
                .select_with_limit(
                    &obs,
                    &legal,
                    &mut rng,
                    SearchLimit::TimeBudget {
                        deadline: Instant::now() + std::time::Duration::from_millis(millis),
                        max_simulations: 100_000,
                    },
                )
                .unwrap()
                .1
                .simulations_completed
        };
        let short = measure(120);
        let long = measure(600);
        assert!(short > 0 && long > 0);
        assert!(
            long >= short,
            "a longer budget must not complete fewer simulations ({short} then {long})"
        );
    }

    #[test]
    fn an_already_spent_budget_returns_a_legal_fallback() {
        let game = Game::new(Config {
            players: 2,
            seed: 5,
            additional_action_space_enabled: false,
        });
        let obs = game.observation(game.public.active_player).unwrap();
        let legal = game.legal_spaces();
        let policy = MctsPolicy {
            simulations: 50,
            tree_depth: 2,
        };
        let mut rng = ChaCha8Rng::seed_from_u64(3);
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
        assert!(outcome.timed_out);
        assert!(outcome.used_fallback);
        assert_eq!(outcome.simulations_completed, 0);
        assert_eq!(outcome.mode, SearchMode::TimeBudgeted);
    }

    #[test]
    fn sampler_obeys_public_state_and_stage_boundaries() {
        let mut game = Game::new(Config::default());
        let obs = game.observation(game.public.active_player).unwrap();
        let mut rng = ChaCha8Rng::seed_from_u64(11);
        for _ in 0..20 {
            let sampled = sample_game(&obs, &mut rng).unwrap();
            assert_eq!(sampled.public, obs.public);
            sampled.validate().unwrap();
            assert_eq!(
                sampled.hidden_round_deck.len(),
                14 - obs.public.revealed.len()
            );
        }
        game.public.revealed.clear(); // changing public info changes what is sampled
        let original = sample_game(&obs, &mut rng).unwrap();
        assert_eq!(original.public.revealed.len(), 1);
    }

    #[test]
    fn sampler_keeps_own_hand_and_never_reads_real_opponent_hand() {
        use crate::cards::{verified_basic_card_set, CardKind};
        use rand::SeedableRng;

        let game = Game::with_cards(
            Config {
                seed: 40,
                ..Config::default()
            },
            verified_basic_card_set(2).unwrap(),
        )
        .unwrap();
        let observed = game.observation(0).unwrap();
        let mut changed_hidden = game.clone();
        let old_index = changed_hidden.cards.hands[1]
            .iter()
            .position(|id| {
                changed_hidden.card_set.definition(id).unwrap().kind == CardKind::MinorImprovement
            })
            .unwrap();
        let replacement = changed_hidden.cards.undealt[0].clone();
        let old = std::mem::replace(&mut changed_hidden.cards.hands[1][old_index], replacement);
        changed_hidden.cards.undealt[0] = old;
        changed_hidden.validate().unwrap();
        assert_eq!(changed_hidden.observation(0).unwrap(), observed);

        let mut first_rng = ChaCha8Rng::seed_from_u64(99);
        let mut second_rng = ChaCha8Rng::seed_from_u64(99);
        for _ in 0..5 {
            let first = sample_game(&observed, &mut first_rng).unwrap();
            let second =
                sample_game(&changed_hidden.observation(0).unwrap(), &mut second_rng).unwrap();
            first.validate().unwrap();
            second.validate().unwrap();
            assert_eq!(first.public, observed.public);
            assert_eq!(first.cards.hands[0], observed.hand);
            assert_eq!(first.cards, second.cards);
            assert_eq!(first.hidden_round_deck, second.hidden_round_deck);
        }
        assert_ne!(game.cards, changed_hidden.cards);
    }

    #[test]
    fn sampler_survives_a_publicly_played_card() {
        use crate::cards::verified_basic_card_set;
        use crate::engine::{Action, Detail};
        use rand::SeedableRng;

        let mut game = Game::with_cards(
            Config {
                seed: 33,
                ..Config::default()
            },
            verified_basic_card_set(2).unwrap(),
        )
        .unwrap();
        let acting = game.public.active_player;
        let card_id = game
            .observation(acting)
            .unwrap()
            .hand
            .into_iter()
            .find(|id| game.card_set.definition(id).unwrap().kind == CardKind::Occupation)
            .unwrap();
        game.apply(Action {
            space: Space::Lessons,
            detail: Detail::PlayOccupation { card_id },
        })
        .unwrap();
        let obs = game.observation(game.public.active_player).unwrap();
        let mut rng = ChaCha8Rng::seed_from_u64(115);
        for _ in 0..3 {
            let simulated = sample_game(&obs, &mut rng).unwrap();
            simulated.validate().unwrap();
            assert_eq!(simulated.public, obs.public);
            assert_eq!(simulated.cards.hands[obs.player_id], obs.hand);
            assert_eq!(
                simulated
                    .cards
                    .hands
                    .iter()
                    .map(Vec::len)
                    .collect::<Vec<_>>(),
                obs.hand_counts
            );
        }
    }

    #[test]
    fn sampled_card_game_can_finish_a_real_rollout() {
        use crate::cards::verified_basic_card_set;
        use rand::SeedableRng;

        let game = Game::with_cards(
            Config {
                seed: 77,
                ..Config::default()
            },
            verified_basic_card_set(2).unwrap(),
        )
        .unwrap();
        let obs = game.observation(game.public.active_player).unwrap();
        let mut rng = ChaCha8Rng::seed_from_u64(5);
        let mut sampled = sample_game(&obs, &mut rng).unwrap();
        rollout(&mut sampled, &mut rng, None).unwrap();
        assert!(sampled.public.finished);
        assert_eq!(sampled.public.round, 14);
        sampled.validate().unwrap();
    }

    #[test]
    fn mcts_can_select_a_legal_move_with_private_cards() {
        use crate::cards::verified_basic_card_set;
        use rand::SeedableRng;

        let game = Game::with_cards(
            Config {
                seed: 52,
                ..Config::default()
            },
            verified_basic_card_set(2).unwrap(),
        )
        .unwrap();
        let obs = game.observation(game.public.active_player).unwrap();
        let legal = game.legal_spaces();
        let policy = MctsPolicy {
            simulations: 4,
            tree_depth: 2,
        };
        let mut rng_a = ChaCha8Rng::seed_from_u64(6);
        let mut rng_b = ChaCha8Rng::seed_from_u64(6);
        let selected = policy.select(&obs, &legal, &mut rng_a);
        assert_eq!(selected, policy.select(&obs, &legal, &mut rng_b));
        assert!(legal.contains(&selected.unwrap()));
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
                players: 2,
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

    #[test]
    fn elapsed_budget_falls_back_to_a_legal_move() {
        let game = Game::new(Config {
            players: 4,
            seed: 9,
            additional_action_space_enabled: false,
        });
        let obs = game.observation(game.public.active_player).unwrap();
        let legal = game.legal_spaces();
        assert!(legal.len() > 1);
        let policy = MctsPolicy {
            simulations: 10_000,
            tree_depth: 2,
        };
        let mut rng = ChaCha8Rng::seed_from_u64(3);
        let (space, outcome) = policy
            .select_with_budget(&obs, &legal, &mut rng, Some(Instant::now()))
            .unwrap();
        assert!(legal.contains(&space), "fallback must stay legal");
        assert!(outcome.timed_out, "an elapsed deadline must be reported");
        assert_eq!(outcome.simulations_completed, 0);
        assert!(outcome.used_fallback);
    }

    #[test]
    fn rollout_stops_when_the_deadline_has_passed() {
        let game = Game::new(Config {
            players: 2,
            seed: 5,
            additional_action_space_enabled: false,
        });
        let mut rng = ChaCha8Rng::seed_from_u64(1);
        let obs = game.observation(game.public.active_player).unwrap();
        let mut sampled = sample_game(&obs, &mut rng).unwrap();
        assert!(
            !rollout(&mut sampled, &mut rng, Some(Instant::now())).unwrap(),
            "an elapsed deadline must abort the playout, not finish it"
        );
    }
}
