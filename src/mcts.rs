//! M3: information-safe, re-determinized Monte Carlo tree search.
//! A fresh hidden round order is sampled from the public revealed cards for
//! each simulation. No Game reference (or hidden RNG) enters SpacePolicy.
use crate::cards::{CardKind, CardSet, CardState};
use crate::evaluation::{DetailAwareEvaluationPolicy, WeightedEvaluationPolicy};
use crate::model::{
    default_event_budget, validate_search_public, Config, Game, Observation, RngStreams, RoundCard,
    Space,
};
use crate::policy::{CardChoiceDetails, DetailPolicy, GreedyPolicy, SpacePolicy};
#[cfg(test)]
use crate::registry::EventKind;
use crate::score::standings_with_cards;
use rand::{seq::SliceRandom, Rng, SeedableRng};
use rand_chacha::ChaCha8Rng;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;
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

    // Standard game: 7 cards per kind per player initially.
    const INITIAL_HAND_PER_KIND: usize = 7;

    // Compute per-kind hand sizes for each player from hand_counts (source of truth).
    // hand_counts[i] = total cards in player i's hand (public information).
    // We know played cards per kind from public state.
    // For the observer, we know the exact hand composition from obs.hand.
    // For opponents, we infer per-kind counts:
    //   expected_per_kind = INITIAL_HAND_PER_KIND.saturating_sub(played_per_kind)
    // If sum(expected) == hand_counts[i], use expected.
    // If sum(expected) < hand_counts[i], distribute extra cards to kinds with remaining pool.
    // If sum(expected) > hand_counts[i], contradiction -> return None.
    //
    // This handles non-standard states (draft variants, card draw effects) while
    // preserving the standard 7+7 deal as the baseline.

    // First, compute played cards per kind for each player.
    let mut played_per_kind = vec![[0usize; 2]; players]; // [Occupation, MinorImprovement]
    for (i, player) in obs.public.players.iter().enumerate() {
        for id in &player.played_cards {
            if let Some(def) = set.definition(id) {
                match def.kind {
                    CardKind::Occupation => played_per_kind[i][0] += 1,
                    CardKind::MinorImprovement => played_per_kind[i][1] += 1,
                }
            }
        }
    }

    // For the observer, compute actual hand composition from obs.hand.
    let mut observer_hand_per_kind = [0usize; 2];
    for id in &obs.hand {
        if let Some(def) = set.definition(id) {
            match def.kind {
                CardKind::Occupation => observer_hand_per_kind[0] += 1,
                CardKind::MinorImprovement => observer_hand_per_kind[1] += 1,
            }
        }
    }

    // Build one shared pool per kind (unknown cards not in the known set).
    let mut pool_occ = set
        .definitions
        .iter()
        .filter(|c| c.kind == CardKind::Occupation)
        .map(|c| c.id.clone())
        .filter(|id| !known.contains(id))
        .collect::<Vec<_>>();
    let mut pool_minor = set
        .definitions
        .iter()
        .filter(|c| c.kind == CardKind::MinorImprovement)
        .map(|c| c.id.clone())
        .filter(|id| !known.contains(id))
        .collect::<Vec<_>>();
    pool_occ.shuffle(rng);
    pool_minor.shuffle(rng);

    // Assign cards to each player's hand per kind.
    for i in 0..players {
        if i == obs.player_id {
            // Observer's hand is already fixed (exact cards known).
            // Just verify the kind counts match hand_counts.
            let total_known = observer_hand_per_kind[0] + observer_hand_per_kind[1];
            if total_known != obs.hand_counts[i] {
                return None;
            }
            continue;
        }

        let total_hand = obs.hand_counts[i];
        let played_occ = played_per_kind[i][0];
        let played_minor = played_per_kind[i][1];

        // Expected hand per kind from standard deal (7 per kind) minus played.
        let expected_occ = INITIAL_HAND_PER_KIND.saturating_sub(played_occ);
        let expected_minor = INITIAL_HAND_PER_KIND.saturating_sub(played_minor);
        let expected_total = expected_occ + expected_minor;

        let (target_occ, target_minor) = if expected_total == total_hand {
            (expected_occ, expected_minor)
        } else if expected_total < total_hand {
            // Extra cards beyond standard deal (e.g., from card draw effects).
            // Distribute extra cards proportionally to kinds with remaining pool capacity.
            let extra = total_hand - expected_total;
            let mut occ = expected_occ;
            let mut minor = expected_minor;
            // Prefer distributing to kinds that still have cards in the pool.
            let occ_capacity = pool_occ.len().saturating_sub(occ);
            let minor_capacity = pool_minor.len().saturating_sub(minor);
            if occ_capacity > 0 && minor_capacity > 0 {
                // Distribute proportionally to capacity.
                let total_cap = occ_capacity + minor_capacity;
                let add_occ = (extra * occ_capacity / total_cap).min(occ_capacity);
                let add_minor = extra - add_occ;
                occ += add_occ;
                minor += add_minor.min(minor_capacity);
            } else if occ_capacity > 0 {
                occ += extra.min(occ_capacity);
            } else if minor_capacity > 0 {
                minor += extra.min(minor_capacity);
            } else {
                // No pool capacity for extra cards -> contradiction.
                return None;
            }
            (occ, minor)
        } else {
            // expected_total > total_hand: played more cards than initially dealt for this kind.
            // This is a contradiction unless cards were removed from hand by some effect.
            // We treat it as irreconcilable.
            return None;
        };

        if pool_occ.len() < target_occ || pool_minor.len() < target_minor {
            return None;
        }
        hands[i].extend(pool_occ.drain(..target_occ));
        hands[i].extend(pool_minor.drain(..target_minor));
    }

    undealt.extend(pool_occ);
    undealt.extend(pool_minor);

    // Validate that the constructed hands match the observed total hand counts.
    // (Observer already validated above; opponents constructed to match.)
    for (i, hand) in hands.iter().enumerate().take(players) {
        if hand.len() != obs.hand_counts[i] {
            return None;
        }
    }

    let cards = CardState { hands, undealt };
    // Use validate_in_game for in-game states (location partition + card score only)
    cards.validate_in_game(&set, &obs.public.players).ok()?;
    Some((set, cards))
}

/// Build a hypothetical position from public information only.
///
/// Public so the profiler (`src/bin/mcts_profile.rs`) can measure the
/// sampling cost separately from the rollout; policies must not use it to look
/// at hidden state.
pub fn sample_game(obs: &Observation, rng: &mut ChaCha8Rng) -> Option<Game> {
    let seed: u64 = rng.gen();
    let (set, cards) = sample_private_cards(obs, rng)?;

    // Build the hidden round deck: all unrevealed cards, shuffled by stage.
    let mut deck = Vec::new();
    for stage in 1..=6 {
        let mut remaining = RoundCard::ALL
            .iter()
            .copied()
            .filter(|c| c.stage() == stage && !obs.public.revealed.contains(c))
            .collect::<Vec<_>>();
        remaining.shuffle(rng);
        deck.extend(remaining);
    }
    deck.reverse(); // Game::start_round pops the next card from the end.

    // Create RngStreams from the sampled seed (matching Game::from_card_state derivation).
    let round_seed = seed ^ 0x38DB_3BE3_DF45_3FAF;
    let action_seed = seed ^ 0x6A09_E667_F3BC_C909;
    let rng_streams = RngStreams {
        round: ChaCha8Rng::seed_from_u64(round_seed),
        ai: ChaCha8Rng::seed_from_u64(action_seed),
    };

    // CardSet from observation's card definitions.
    let card_set = Arc::new(set);

    // well_food_schedule from observation (added in PR #265).
    // delayed_well mirrors well_food_schedule; use the same data for both.
    let well_food_schedule = obs.well_food_schedule.clone();
    let delayed_well = well_food_schedule.clone();

    // card_uses and card_use_limits from observation (P2-3).
    // card_uses tracks per-card use counts in the current round (public info).
    // card_use_limits is DERIVED from the card definitions (CardDefinition::use_limit),
    // not copied from the observation: obs.card_use_limits is a convenience mirror
    // (#[serde(default)] => {} for pre-mirror saves), while card_definitions is
    // always complete. Deriving here means a save/restore that dropped the mirror
    // still reconstructs a Game with correct per-round limits, matching how
    // Game::from_card_state builds card_use_limits at startup.
    let card_uses = obs.card_uses.clone();
    let card_use_limits = obs
        .card_definitions
        .iter()
        .filter_map(|card| card.use_limit.map(|limit| (card.id.clone(), limit)))
        .collect();

    // Search positions are hypothetical: skip state hashing and event logging.
    let config = Config {
        seed,
        additional_action_space_enabled: obs.public.additional_action_space_enabled,
        players: obs.public.players.len(),
    };

    let game = Game::search_from_parts(
        config,
        obs.public.clone(),
        deck,
        rng_streams,
        delayed_well,
        well_food_schedule,
        card_set,
        cards,
        None,                   // pending
        default_event_budget(), // event_budget
        card_uses,
        card_use_limits,
        0, // next_decision_id
    );

    // Validate the constructed position (public invariants only, no private validate).
    validate_search_public(
        &game.public,
        game.config.players,
        game.config.additional_action_space_enabled,
        game.card_set.enabled(),
    )
    .ok()?;

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

/// Play one hypothetical game to the end with the search's playout policy.
/// Public for the profiler so the rollout cost can be measured on its own.
pub fn rollout(
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
/// pooled: `SimulationCapped` caps the number of simulation attempts
/// (reproducible), while `TimeBudgeted` fixes the wall-clock cost (not
/// reproducible).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum SearchMode {
    /// Cap the search at N simulation *attempts*. No wall-clock limit is
    /// applied, so a fixed RNG seed reproduces the same choice. The number of
    /// **completed** simulations can be lower than N: a sampled simulation that
    /// turns out invalid is discarded and does not update the tree.
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
    /// Stop after at most this many simulation attempts (`max_simulations`).
    /// Completed simulations can be fewer; see [`SearchMode::SimulationCapped`].
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

    /// Property test: current `sample_game` output distribution is fixed for a
    /// given observation and seed. This digest must stay identical after the
    /// Phase C replacement (SearchPosition direct construction).
    #[test]
    fn sample_game_distribution_digest_is_stable() {
        use crate::cards::verified_basic_card_set;
        use sha2::{Digest, Sha256};
        let game = Game::with_cards(
            Config {
                seed: 42,
                ..Config::default()
            },
            verified_basic_card_set(2).unwrap(),
        )
        .unwrap();
        let obs = game.observation(game.public.active_player).unwrap();
        let mut rng = ChaCha8Rng::seed_from_u64(0xCAFE_BABE_DEAD_BEEF);
        let mut hasher = Sha256::new();
        for _ in 0..64 {
            let sampled = sample_game(&obs, &mut rng).expect("sample_game must succeed");
            // Hash only the hidden partition + round deck; public is identical.
            let data = serde_json::to_vec(&(
                &sampled.cards.hands,
                &sampled.cards.undealt,
                &sampled.hidden_round_deck,
            ))
            .expect("serialize sampled");
            hasher.update(&data);
        }
        let digest = format!("{:x}", hasher.finalize());
        // This value was recorded from the current implementation.
        // DO NOT CHANGE unless the implementation is intentionally replaced.
        // Updated: added A077 Hod to verified_immediate_resource_cards (pool size 30->31)
        // Updated: added A051 Drift-Net Boat to verified_minor_worker_cards (pool size 31->32)
        // Updated: added A122 Pot-Hash Pfannkuchen to verified_worker_bonus_cards (pool size 32->33)
        // Updated: added E007/E008 to verified_immediate_resource_cards — E-prefixed cards are
        // excluded from the verified pool, so the sample_game distribution is unchanged.
        // Updated: added A042 Forest Lake Hut to verified_minor_worker_cards (pool size 33->34)
        // Updated: added A121 Brick Stamping to verified_minor_worker_cards (pool size 34->35)
        assert_eq!(
            digest, "a024ed2ee08bad2a70262613afa86f07d89686e4f28cb17c0a6415d700104e99",
            "sample_game distribution changed — update this test only if intentional"
        );
    }

    /// Compare two observations for public-information equality (shared by the
    /// sample_game acceptance tests below).
    fn obs_eq_public(a: &Observation, b: &Observation) {
        assert_eq!(a.player_id, b.player_id);
        assert_eq!(a.public, b.public);
        assert_eq!(a.hand, b.hand);
        assert_eq!(a.hand_counts, b.hand_counts);
        assert_eq!(a.card_definitions, b.card_definitions);
        assert_eq!(a.well_food_schedule, b.well_food_schedule);
        assert_eq!(a.card_uses, b.card_uses);
        assert_eq!(a.card_use_limits, b.card_use_limits);
    }

    /// Acceptance (c) + roundtrip: sample_game reconstructs a game whose
    /// Observation matches the original across several seeds and in-progress
    /// rounds. The loop really plays cards (Lessons -> PlayOccupation), so the
    /// reconstructed states include non-empty `played_cards`.
    #[test]
    fn sample_game_reconstructs_observation_roundtrip() {
        use crate::cards::verified_basic_card_set;
        use crate::engine::{Action, Detail};
        use rand::SeedableRng;
        use rand_chacha::ChaCha8Rng;

        // Test multiple seeds and multiple rounds.
        for seed in [1, 42, 99, 123, 456] {
            let mut game = Game::with_cards(
                Config {
                    seed,
                    ..Config::default()
                },
                verified_basic_card_set(2).unwrap(),
            )
            .unwrap();
            let mut played_cards_seen = false;

            // Play a few rounds to create in-progress states.
            for round in 1..=5 {
                // Get observation for current player and test roundtrip.
                let acting = game.public.active_player;
                let obs = game.observation(acting).unwrap();
                let mut rng = ChaCha8Rng::seed_from_u64(seed * 1000 + round);
                let sampled = sample_game(&obs, &mut rng).expect("sample_game must succeed");
                let reconstructed_obs = sampled.observation(acting).unwrap();
                obs_eq_public(&obs, &reconstructed_obs);
                if obs
                    .public
                    .players
                    .iter()
                    .any(|p| !p.played_cards.is_empty())
                {
                    played_cards_seen = true;
                }

                // Take a legal action to advance the game. Prefer really playing
                // a cost-free occupation at Lessons so `played_cards` grows;
                // otherwise fall back to DayLaborer or the first legal space.
                let legal = game.legal_spaces();
                if legal.is_empty() {
                    break;
                }
                let mut applied = false;
                if legal.contains(&Space::Lessons) {
                    let playable = obs.hand.iter().find(|id| {
                        game.card_set
                            .definition(id)
                            .is_some_and(|c| c.kind == CardKind::Occupation && c.cost.is_empty())
                    });
                    if let Some(card_id) = playable {
                        let play = Action {
                            space: Space::Lessons,
                            detail: Detail::PlayOccupation {
                                card_id: card_id.clone(),
                            },
                        };
                        if game.apply(play).is_ok() {
                            applied = true;
                        }
                    }
                }
                if !applied {
                    // A card-specific refusal (cost/prerequisite) must not end
                    // the loop: fall back to a plain legal action.
                    let space = legal
                        .iter()
                        .find(|&&s| s == Space::DayLaborer)
                        .copied()
                        .unwrap_or(legal[0]);
                    if game
                        .apply(Action {
                            space,
                            detail: Detail::None,
                        })
                        .is_err()
                    {
                        break;
                    }
                }
            }
            assert!(
                played_cards_seen,
                "seed {seed}: the loop must reach a state with played cards"
            );
        }
    }

    /// Acceptance (a): a hand that grew beyond the standard `14 - played` count
    /// (an undealt card moved into a hand through [`CardZone::Hand`]) must
    /// reconstruct, because `hand_counts` is the information source, not a
    /// fixed 7 per kind. The previous fixed-7 reconstruction returned `None`
    /// for exactly this state.
    #[test]
    fn sample_game_restores_a_hand_larger_than_the_standard_deal() {
        use crate::card_zones::{move_card, validate_card_partition, CardMoveCtx, CardZone};
        use crate::cards::verified_basic_card_set;
        use rand::SeedableRng;
        use rand_chacha::ChaCha8Rng;

        let mut game = Game::with_cards(
            Config {
                seed: 8,
                ..Config::default()
            },
            verified_basic_card_set(2).unwrap(),
        )
        .unwrap();

        // Move one undealt occupation into player 1's hand. This is the engine's
        // own hand mutation (`hands[player].push` via move_card), so the running
        // invariants hold while hand_counts[1] leaves the standard 7+7 deal.
        let acquired = game
            .cards
            .undealt
            .iter()
            .find(|id| {
                game.card_set
                    .definition(id)
                    .is_some_and(|c| c.kind == CardKind::Occupation)
            })
            .cloned()
            .expect("the verified pool keeps undealt occupations");
        {
            let mut ctx = CardMoveCtx {
                cards: &mut game.cards,
                players: &mut game.public.players,
                improvement_market: &mut game.public.improvement_market,
            };
            move_card(
                &mut ctx,
                &acquired,
                CardZone::Undealt,
                CardZone::Hand { player: 1 },
            )
            .unwrap();
        }
        validate_card_partition(&game.card_set, &game.cards, &game.public.players).unwrap();

        let obs = game.observation(0).unwrap();
        assert_eq!(obs.hand_counts[1], 15, "the hand grew beyond 14 - played");
        assert_eq!(obs.public.players[1].played_cards.len(), 0);

        let mut rng = ChaCha8Rng::seed_from_u64(0xABCD_1234);
        let sampled = sample_game(&obs, &mut rng).expect("a grown hand must reconstruct");
        let reconstructed = sampled.observation(0).unwrap();
        obs_eq_public(&obs, &reconstructed);
        assert_eq!(
            sampled.cards.hands.iter().map(Vec::len).collect::<Vec<_>>(),
            obs.hand_counts,
            "the sampled hands must match the observed hand counts"
        );
        // The sampled position only owes the running invariants: the 7+7 count
        // is a setup property this legitimately-acquired card breaks.
        validate_card_partition(&sampled.card_set, &sampled.cards, &sampled.public.players)
            .unwrap();
    }

    /// Acceptance (b): a state with eight played occupations in front of one
    /// player (played > 7 for a single kind) must sample instead of being
    /// discarded, and the policy must turn it into completed simulations. The
    /// previous `checked_sub(7 - played)` reconstruction returned `None` here,
    /// which made `MctsPolicy` throw away every simulation.
    #[test]
    fn sample_game_survives_many_played_cards_and_the_policy_simulates() {
        use crate::card_zones::{move_card, validate_card_partition, CardMoveCtx, CardZone};
        use crate::cards::verified_basic_card_set;
        use rand::SeedableRng;
        use rand_chacha::ChaCha8Rng;

        let mut game = Game::with_cards(
            Config {
                seed: 7,
                ..Config::default()
            },
            verified_basic_card_set(2).unwrap(),
        )
        .unwrap();

        // Give the opponent an eighth occupation from the undealt pool, then
        // move all eight occupations from their hand to their played area —
        // a public state the engine can reach (play + acquisition), where
        // played(Occupation) = 8 exceeds the fixed 7.
        let eighth = game
            .cards
            .undealt
            .iter()
            .find(|id| {
                game.card_set
                    .definition(id)
                    .is_some_and(|c| c.kind == CardKind::Occupation)
            })
            .cloned()
            .expect("the verified pool keeps undealt occupations");
        {
            let mut ctx = CardMoveCtx {
                cards: &mut game.cards,
                players: &mut game.public.players,
                improvement_market: &mut game.public.improvement_market,
            };
            move_card(
                &mut ctx,
                &eighth,
                CardZone::Undealt,
                CardZone::Hand { player: 1 },
            )
            .unwrap();
        }
        // Now the opponent's hand holds eight occupations: move them all to the
        // played area through the canonical zone API.
        let played: Vec<String> = game.cards.hands[1]
            .iter()
            .filter(|id| {
                game.card_set
                    .definition(id)
                    .is_some_and(|c| c.kind == CardKind::Occupation)
            })
            .cloned()
            .collect();
        {
            let mut ctx = CardMoveCtx {
                cards: &mut game.cards,
                players: &mut game.public.players,
                improvement_market: &mut game.public.improvement_market,
            };
            for id in &played {
                move_card(
                    &mut ctx,
                    id,
                    CardZone::Hand { player: 1 },
                    CardZone::Played { player: 1 },
                )
                .unwrap();
            }
        }
        // Keep the card-points cache consistent with the direct zone move.
        let points: i32 = played
            .iter()
            .filter_map(|id| game.card_set.definition(id))
            .map(|c| c.points)
            .sum();
        game.public.players[1].card_points = points;
        validate_card_partition(&game.card_set, &game.cards, &game.public.players).unwrap();

        let obs = game.observation(0).unwrap();
        assert_eq!(
            played.len(),
            8,
            "the opponent really played eight occupations"
        );
        assert_eq!(obs.hand_counts[1], 7, "hand = 14 + 1 acquired - 8 played");
        assert_eq!(
            obs.public.players[1]
                .played_cards
                .iter()
                .filter(|id| game
                    .card_set
                    .definition(id)
                    .is_some_and(|c| c.kind == CardKind::Occupation))
                .count(),
            8
        );

        let mut rng = ChaCha8Rng::seed_from_u64(0x0F0F_0F0F);
        let sampled = sample_game(&obs, &mut rng)
            .expect("a state with played > 7 must sample, not return None");
        obs_eq_public(&obs, &sampled.observation(0).unwrap());
        validate_card_partition(&sampled.card_set, &sampled.cards, &sampled.public.players)
            .unwrap();

        // The policy must convert this state into simulations instead of the
        // simulations_completed = 0 fallback.
        let legal = game.legal_spaces();
        let policy = MctsPolicy {
            simulations: 6,
            tree_depth: 2,
        };
        let (_space, outcome) = policy
            .select_with_limit(&obs, &legal, &mut rng, SearchLimit::Simulations(6))
            .expect("the policy must select a legal move");
        assert!(
            outcome.simulations_completed > 0,
            "played >= 8 must feed the search instead of discarding every simulation"
        );
    }

    /// Test with well reservations (delayed_well / well_food_schedule).
    #[test]
    fn sample_game_reconstructs_well_reservations() {
        use crate::cards::verified_basic_card_set;
        use rand::SeedableRng;
        use rand_chacha::ChaCha8Rng;

        let mut game = Game::with_cards(
            Config {
                seed: 77,
                ..Config::default()
            },
            verified_basic_card_set(2).unwrap(),
        )
        .unwrap();

        // Install a reservation for round 3 in the exact shape a real Well
        // purchase produces (both maps written together, one recipient). A full
        // purchase path needs resource setup that is irrelevant to what this
        // test pins: sample_game must carry the schedule across the rebuild.
        game.delayed_well.entry(3).or_default().push(0);
        game.well_food_schedule.entry(3).or_default().push(0);

        let obs = game.observation(0).unwrap();
        assert!(obs.well_food_schedule.contains_key(&3));

        let mut rng = ChaCha8Rng::seed_from_u64(0xDEAD_BEEF);
        let sampled = sample_game(&obs, &mut rng).expect("sample_game must succeed");
        let reconstructed_obs = sampled.observation(0).unwrap();

        assert_eq!(reconstructed_obs.well_food_schedule, obs.well_food_schedule);
        assert_eq!(sampled.delayed_well, game.delayed_well);
        assert_eq!(sampled.well_food_schedule, game.well_food_schedule);
    }

    /// Acceptance (e): a synthetic `use_limit` card that is really played and
    /// really used this round (card_uses non-empty and exactly at its limit)
    /// must reconstruct with identical card_uses and limits. The sampled game's
    /// card_use_limits is derived from the card definitions, not copied from
    /// the observation mirror.
    #[test]
    fn sample_game_reconstructs_card_use_limits() {
        use crate::cards::{verified_basic_card_set, CardDefinition, CardEffect, CardSet};
        use crate::engine::{Action, Detail};
        use crate::model::Good;
        use rand::SeedableRng;
        use rand_chacha::ChaCha8Rng;

        // Synthetic card: fires on DayLaborer, at most once per round.
        let mut definitions = verified_basic_card_set(2).unwrap().definitions;
        definitions.push(CardDefinition {
            id: "TEST_LIMITED".into(),
            kind: CardKind::Occupation,
            cost: vec![],
            points: 0,
            effects: vec![CardEffect::AfterWorkerGain {
                space: Space::DayLaborer,
                good: Good::Wood,
                amount: 1,
            }],
            use_limit: Some(1),
        });
        let card_set = CardSet { definitions };

        // Explicit hands put TEST_LIMITED in player 0's hand and make them the
        // start player, so the real engine play path is exercised.
        let mut occupations: Vec<String> = card_set
            .definitions
            .iter()
            .filter(|c| c.kind == CardKind::Occupation && c.id != "TEST_LIMITED")
            .map(|c| c.id.clone())
            .collect();
        let minors: Vec<String> = card_set
            .definitions
            .iter()
            .filter(|c| c.kind == CardKind::MinorImprovement)
            .map(|c| c.id.clone())
            .collect();
        let first_hand = [
            vec!["TEST_LIMITED".to_string()],
            occupations.drain(..6).collect(),
            minors[..7].to_vec(),
        ]
        .concat();
        let second_hand = [occupations.drain(..7).collect(), minors[7..14].to_vec()].concat();
        let mut game = Game::with_cards_and_hands_with_start_player(
            Config {
                seed: 88,
                ..Config::default()
            },
            card_set,
            vec![first_hand, second_hand],
            0,
        )
        .unwrap();

        // Round 1: play the limited card, let player 1 take a different space
        // (a space is usable once per round, so player 0 must not take
        // DayLaborer for them), then trigger the card once on DayLaborer. The
        // use ledger is still inside round 1, so card_uses is non-empty and
        // exactly at the declared limit.
        game.apply(Action {
            space: Space::Lessons,
            detail: Detail::PlayOccupation {
                card_id: "TEST_LIMITED".into(),
            },
        })
        .unwrap();
        game.apply(Action {
            space: Space::GrainSeeds,
            detail: Detail::None,
        })
        .unwrap();
        game.apply(Action {
            space: Space::DayLaborer,
            detail: Detail::None,
        })
        .unwrap();

        let obs = game.observation(0).unwrap();
        assert_eq!(
            obs.card_uses
                .get(&format!("TEST_LIMITED:{:?}", EventKind::WorkerPlaced)),
            Some(&1),
            "the card was really used once this round"
        );
        assert_eq!(obs.card_use_limits.get("TEST_LIMITED"), Some(&1));

        let mut rng = ChaCha8Rng::seed_from_u64(0xBEEF_CAFE);
        let sampled = sample_game(&obs, &mut rng).expect("sample_game must succeed");
        let reconstructed_obs = sampled.observation(0).unwrap();

        obs_eq_public(&obs, &reconstructed_obs);
        assert_eq!(sampled.card_uses, game.card_uses);
        assert_eq!(
            sampled.card_use_limits, game.card_use_limits,
            "limits must derive from the card definitions identically"
        );
        sampled.validate().unwrap();

        // P2-A: the mirror is a convenience copy behind `#[serde(default)]`, so
        // a pre-mirror save yields an empty Observation mirror while the
        // definitions still declare the limit. Deriving from card_definitions
        // must still rebuild a Game with the declared limits.
        let mut stripped = obs.clone();
        stripped.card_use_limits.clear();
        let mut rng2 = ChaCha8Rng::seed_from_u64(0xCAFE_0001);
        let sampled_stripped =
            sample_game(&stripped, &mut rng2).expect("a mirror-less observation must sample");
        assert_eq!(
            sampled_stripped.card_use_limits.get("TEST_LIMITED"),
            Some(&1),
            "limits must come from CardDefinition::use_limit, not the observation mirror"
        );
    }

    /// Test that start_round transition preserves public rewards for well and
    /// limited cards (card_uses cleared, well_food_schedule mirror maintained).
    #[test]
    fn start_round_preserves_well_and_card_limits() {
        use crate::cards::verified_basic_card_set;
        use rand::SeedableRng;
        use rand_chacha::ChaCha8Rng;

        let mut game = Game::with_cards(
            Config {
                seed: 99,
                ..Config::default()
            },
            verified_basic_card_set(2).unwrap(),
        )
        .unwrap();

        // Set up well reservation for round 2.
        game.delayed_well.entry(2).or_default().push(0);
        game.well_food_schedule.entry(2).or_default().push(0);

        // Set up some card_uses (simulating a card was used this round).
        game.card_uses
            .insert(format!("TEST_CARD:{:?}", EventKind::RoundStart), 1);

        // Save card_use_limits before start_round.
        let card_use_limits_before = game.card_use_limits.clone();

        // Advance to round 2.
        game.start_round();
        assert_eq!(game.public.round, 2);

        // Well food should have been granted.
        // Player 0 is start player with 2 initial food + 1 from well = 3.
        assert_eq!(game.public.players[0].supply.food, 2 + 1);

        // delayed_well and well_food_schedule for round 2 should be consumed.
        assert!(!game.delayed_well.contains_key(&2));
        assert!(!game.well_food_schedule.contains_key(&2));

        // card_uses should be cleared at round boundary.
        assert!(game.card_uses.is_empty());

        // card_use_limits should persist unchanged.
        assert_eq!(game.card_use_limits, card_use_limits_before);

        // Acceptance (d) + round boundary: the post-start_round state round-trips
        // through sample_game — supply, public state and every Observation field
        // (well mirror consumed, card_uses cleared, limits intact) must match.
        let obs = game.observation(0).unwrap();
        assert!(obs.well_food_schedule.is_empty());
        let mut rng = ChaCha8Rng::seed_from_u64(0x5EED_0002);
        let sampled =
            sample_game(&obs, &mut rng).expect("the post-start_round state must reconstruct");
        obs_eq_public(&obs, &sampled.observation(0).unwrap());
        assert_eq!(sampled.public.players[0].supply.food, 2 + 1);
        sampled.validate().unwrap();
    }

    /// P3 invariant: a legacy save can carry a well_food_schedule entry whose
    /// delayed_well entry is already gone. start_round must still drop the
    /// schedule entry for the consumed round instead of feeding the orphan
    /// mirror forever, while a real paired reservation still grants its food.
    #[test]
    fn start_round_drops_an_orphan_well_schedule_entry() {
        use crate::cards::verified_basic_card_set;

        let mut game = Game::with_cards(
            Config {
                seed: 101,
                ..Config::default()
            },
            verified_basic_card_set(2).unwrap(),
        )
        .unwrap();
        // Orphan: schedule entry without a delayed_well twin (legacy save shape).
        game.well_food_schedule.entry(2).or_default().push(1);

        game.start_round();
        assert_eq!(game.public.round, 2);
        assert!(
            !game.well_food_schedule.contains_key(&2),
            "the orphan schedule entry must be dropped unconditionally"
        );
        assert!(
            !game.delayed_well.contains_key(&2),
            "nothing may appear in delayed_well for a schedule-only entry"
        );
        // No grant may have fired from the orphan: both players keep their
        // initial food (start player 2, other 3).
        assert_eq!(game.public.players[0].supply.food, 2);
        assert_eq!(game.public.players[1].supply.food, 3);
    }
}
