//! Design-aligned evaluation harness support (PR #400 `docs/ai-evaluation-design.md` §1.7, §2, §3, §5).
//!
//! The seat-rotation selection/search code lives here instead of in
//! `bin/paired_eval.rs` so the legacy A/B binary and the design-aligned
//! `bin/eval_harness.rs` cannot drift apart. Moving it changed no behaviour:
//! `paired_eval` imports the same types and functions from this module.
//!
//! This module also owns the **test-fixture** 10-7 pool and the draft runner
//! used by the harness. The fixture pool exists because the official 4-player
//! gate (G1: 40 verified occupations + 40 verified minor improvements) is not
//! met: `card_pool_report` currently counts zero officially verified cards.
//! Fixture games exercise the draft→game wiring; they are never a strength or
//! official-rules claim (design §4.1).

use crate::cards::{CardDefinition, CardKind as DefinitionKind, CardSet};
use crate::draft::{
    cpu_pick, replay_draft, Direction, DraftMode, DraftPool, DraftRuleset, DraftSetup, DraftState,
};
use crate::draft_ai::{decision_rng, DraftAiKind};
use crate::draft_game::card_catalog;
use crate::evaluation::{EvaluationPolicy, VariantEvaluationPolicy};
use crate::mcts::{MctsOutcome, MctsPolicy, SearchLimit, SearchMode};
use crate::mcts_puct::{LeafMode, PuctConfig, PuctPolicy};
use crate::model::{Observation, Space};
use crate::policy::{GreedyPolicy, RandomPolicy, SpacePolicy};
use rand_chacha::ChaCha8Rng;
use serde_json::{json, Value};
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::time::{Duration, Instant};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ai {
    Random,
    Greedy,
    Eval,
    EvalVeg,
    EvalFood,
    EvalVegConditional,
    EvalRoom,
    Mcts,
    Puct,
}

impl Ai {
    pub fn parse(name: &str) -> Result<Self, String> {
        match name {
            "random" => Ok(Ai::Random),
            "greedy" => Ok(Ai::Greedy),
            "eval" => Ok(Ai::Eval),
            "eval_veg" => Ok(Ai::EvalVeg),
            "eval_food" => Ok(Ai::EvalFood),
            "eval_veg_cond" => Ok(Ai::EvalVegConditional),
            "eval_room" => Ok(Ai::EvalRoom),
            "mcts" => Ok(Ai::Mcts),
            "puct" => Ok(Ai::Puct),
            _ => Err(format!("unsupported AI: {name}")),
        }
    }
    pub fn name(self) -> &'static str {
        match self {
            Ai::Random => "random",
            Ai::Greedy => "greedy",
            Ai::Eval => "eval",
            Ai::EvalVeg => "eval_veg",
            Ai::EvalFood => "eval_food",
            Ai::EvalVegConditional => "eval_veg_cond",
            Ai::EvalRoom => "eval_room",
            Ai::Mcts => "mcts",
            Ai::Puct => "puct",
        }
    }
    pub fn is_search(self) -> bool {
        matches!(self, Ai::Mcts | Ai::Puct)
    }
}

/// A stable identity for one PUCT configuration (#219). Historical runs all
/// logged the policy name `puct`, so two very different configurations are
/// indistinguishable in the archives; every new record carries these IDs so
/// an ablation never compares two unnamed variants.
pub fn puct_config_id(config: &PuctConfig) -> String {
    let leaf = match config.leaf_mode {
        LeafMode::Heuristic => "heuristic",
        LeafMode::Terminal => "terminal",
    };
    format!(
        "puct-s{}-d{}-c{}-pt{}-vt{}-{}-rl{}-fp{}",
        config.simulations,
        config.tree_depth,
        config.c_puct,
        config.prior_temperature,
        config.value_temperature,
        leaf,
        config.root_candidate_limit,
        config.final_prior_weight
    )
}

/// Per-decision search settings shared by every seat (#164).
///
/// The fixed simulation count is **per policy**: MCTS and PUCT each keep the
/// value the caller configured, so a PUCT focus is never silently run with
/// MCTS's count (and the recorded config never disagrees with what ran).
#[derive(Clone, Copy, Debug)]
pub struct SearchSettings {
    pub mode: SearchMode,
    pub mcts_simulations: usize,
    pub puct_simulations: usize,
    pub budget: Duration,
    pub time_budget_max_simulations: usize,
}

impl SearchSettings {
    pub fn rule_for(self, ai: Ai) -> Option<SearchRule> {
        let fixed = match ai {
            Ai::Mcts => self.mcts_simulations,
            Ai::Puct => self.puct_simulations,
            Ai::Random
            | Ai::Greedy
            | Ai::Eval
            | Ai::EvalVeg
            | Ai::EvalFood
            | Ai::EvalVegConditional
            | Ai::EvalRoom => return None,
        };
        Some(match self.mode {
            SearchMode::SimulationCapped => SearchRule::Simulations(fixed),
            SearchMode::TimeBudgeted => SearchRule::TimeBudget {
                budget: self.budget,
                max_simulations: self.time_budget_max_simulations,
            },
        })
    }

    /// The stopping rule for one decision (#167 A).
    ///
    /// `search_deadline` comes from the whole-decision budget and is the
    /// decision deadline minus the detail reserve. It must override the
    /// template clock: using the full budget here was the defect this function
    /// fixes, because the search then consumed the reserve and every decision
    /// ended after its own budget with the detail policy skipped.
    pub fn limit_for(self, ai: Ai, search_deadline: Option<Instant>) -> Option<SearchLimit> {
        Some(self.rule_for(ai)?.resolve(search_deadline))
    }
}

#[derive(Default)]
pub struct FocusOutcomes {
    pub timeouts: u32,
    pub fallbacks: u32,
    pub simulations: u64,
    /// Largest deadline overshoot reported by the search itself.
    pub overshoot_ms: u64,
    /// Largest search-internal decision time reported by the search itself.
    pub elapsed_ms: u64,
}

pub struct SeatSelector<'a> {
    pub focus: usize,
    pub focus_ai: Ai,
    pub opponent_ai: Ai,
    pub random: &'a RandomPolicy,
    pub greedy: &'a GreedyPolicy,
    pub eval: &'a EvaluationPolicy,
    pub eval_veg: &'a VariantEvaluationPolicy,
    pub eval_food: &'a VariantEvaluationPolicy,
    pub eval_veg_cond: &'a VariantEvaluationPolicy,
    pub eval_room: &'a VariantEvaluationPolicy,
    pub mcts: &'a MctsPolicy,
    /// PUCT configuration of the seat in this slot. The focus seat receives
    /// the candidate configuration in the candidate arm and the frozen
    /// baseline configuration in the baseline arm (#219); every opponent PUCT
    /// seat always receives the frozen baseline configuration.
    pub puct: &'a PuctPolicy,
    /// Frozen baseline PUCT configuration used by opponent PUCT seats.
    pub puct_baseline: &'a PuctPolicy,
    /// Stopping rule for the focus seat. `None` for heuristic policies and for
    /// the opponent seats, which always run the policy's own deterministic
    /// default.
    pub search: Option<SearchSettings>,
    pub decision_ns: RefCell<Vec<u64>>,
    pub focus_outcomes: RefCell<FocusOutcomes>,
}

/// Per-decision stopping rule template (#164). The absolute deadline is
/// resolved at the moment the decision starts.
#[derive(Clone, Copy, Debug)]
pub enum SearchRule {
    Simulations(usize),
    TimeBudget {
        budget: Duration,
        max_simulations: usize,
    },
}

impl SearchRule {
    /// Resolve the template into a search limit. `search_deadline` is the
    /// whole-decision budget's search window and wins over the template clock,
    /// so the search stops before the detail reserve is touched (#167 A).
    pub fn resolve(self, search_deadline: Option<Instant>) -> SearchLimit {
        match self {
            SearchRule::Simulations(n) => SearchLimit::Simulations(n),
            SearchRule::TimeBudget {
                budget,
                max_simulations,
            } => SearchLimit::TimeBudget {
                deadline: search_deadline.unwrap_or_else(|| Instant::now() + budget),
                max_simulations,
            },
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub fn select_ai(
    ai: Ai,
    random: &RandomPolicy,
    greedy: &GreedyPolicy,
    eval: &EvaluationPolicy,
    eval_veg: &VariantEvaluationPolicy,
    eval_food: &VariantEvaluationPolicy,
    eval_veg_cond: &VariantEvaluationPolicy,
    eval_room: &VariantEvaluationPolicy,
    mcts: &MctsPolicy,
    puct: &PuctPolicy,
    obs: &Observation,
    legal: &[Space],
    rng: &mut ChaCha8Rng,
    search: Option<SearchLimit>,
) -> (Option<Space>, MctsOutcome) {
    match ai {
        Ai::Random => (random.select(obs, legal, rng), MctsOutcome::default()),
        Ai::Greedy => (greedy.select(obs, legal, rng), MctsOutcome::default()),
        Ai::Eval => (eval.select(obs, legal, rng), MctsOutcome::default()),
        Ai::EvalVeg => (eval_veg.select(obs, legal, rng), MctsOutcome::default()),
        Ai::EvalFood => (eval_food.select(obs, legal, rng), MctsOutcome::default()),
        Ai::EvalVegConditional => (
            eval_veg_cond.select(obs, legal, rng),
            MctsOutcome::default(),
        ),
        Ai::EvalRoom => (eval_room.select(obs, legal, rng), MctsOutcome::default()),
        Ai::Mcts => {
            let limit = search.unwrap_or(SearchLimit::Simulations(mcts.simulations));
            match mcts.select_with_limit(obs, legal, rng, limit) {
                Some((space, outcome)) => (Some(space), outcome),
                None => (None, MctsOutcome::default()),
            }
        }
        Ai::Puct => {
            let limit = search.unwrap_or(SearchLimit::Simulations(puct.config.simulations));
            match puct.select_with_limit(obs, legal, rng, limit) {
                Some((space, outcome)) => (Some(space), outcome),
                None => (None, MctsOutcome::default()),
            }
        }
    }
}

impl SeatSelector<'_> {
    fn choose(
        &self,
        obs: &Observation,
        legal: &[Space],
        rng: &mut ChaCha8Rng,
        search_deadline: Option<Instant>,
    ) -> Option<Space> {
        let is_focus = obs.player_id == self.focus;
        let search = if is_focus {
            self.search
                .and_then(|settings| settings.limit_for(self.focus_ai, search_deadline))
        } else {
            None
        };
        let before = Instant::now();
        let (selected, outcome) = if is_focus {
            select_ai(
                self.focus_ai,
                self.random,
                self.greedy,
                self.eval,
                self.eval_veg,
                self.eval_food,
                self.eval_veg_cond,
                self.eval_room,
                self.mcts,
                self.puct,
                obs,
                legal,
                rng,
                search,
            )
        } else {
            select_ai(
                self.opponent_ai,
                self.random,
                self.greedy,
                self.eval,
                self.eval_veg,
                self.eval_food,
                self.eval_veg_cond,
                self.eval_room,
                self.mcts,
                // Opponent PUCT seats always play the frozen baseline
                // configuration, whatever the candidate arm explores (#219).
                self.puct_baseline,
                obs,
                legal,
                rng,
                None,
            )
        };
        let ns = before.elapsed().as_nanos().min(u128::from(u64::MAX)) as u64;
        self.decision_ns.borrow_mut().push(ns);
        if is_focus {
            let mut outcomes = self.focus_outcomes.borrow_mut();
            if self.focus_ai.is_search() {
                // `timed_out` means the wall-clock deadline ended the decision,
                // which is the normal case for a time-budgeted run.
                if outcome.timed_out {
                    outcomes.timeouts += 1;
                }
                if outcome.used_fallback {
                    outcomes.fallbacks += 1;
                }
                outcomes.simulations += outcome.simulations_completed as u64;
                outcomes.overshoot_ms = outcomes.overshoot_ms.max(outcome.overshoot_ms);
                outcomes.elapsed_ms = outcomes.elapsed_ms.max(outcome.elapsed_ms);
            }
        }
        selected
    }
}

impl SpacePolicy for SeatSelector<'_> {
    fn select(&self, obs: &Observation, legal: &[Space], rng: &mut ChaCha8Rng) -> Option<Space> {
        self.choose(obs, legal, rng, None)
    }

    /// The focus seat searches only inside the budget's search window, so the
    /// detail generation gets its reserve (#167 A).
    fn select_with_deadline(
        &self,
        obs: &Observation,
        legal: &[Space],
        rng: &mut ChaCha8Rng,
        deadline: Option<Instant>,
    ) -> Option<Space> {
        self.choose(obs, legal, rng, deadline)
    }
}

pub fn first_credit(ranks: &[usize], seat: usize) -> Result<f64, String> {
    let tied = ranks.iter().filter(|&&rank| rank == 1).count();
    if tied == 0 || seat >= ranks.len() {
        return Err("invalid standings".into());
    }
    Ok(if ranks[seat] == 1 {
        1.0 / tied as f64
    } else {
        0.0
    })
}

// ---------------------------------------------------------------------------
// 10-7 draft fixture (functional verification only)
// ---------------------------------------------------------------------------

/// Cards dealt per kind and per seat by `draft_10_7_simultaneous`.
const DRAFT_107_DEALT_PER_KIND: usize = 10;
/// Final hand size per kind after a 10-7 draft.
const DRAFT_107_HAND_PER_KIND: usize = 7;
/// A four-player 10-7 draft deals this many cards of each kind in total.
pub const FIXTURE_DRAFT_POOL_PER_KIND: usize = DRAFT_107_DEALT_PER_KIND * 4;

/// Synthetic 40+40 pool used by the harness.
///
/// The ids are prefixed `FIX-` and the definitions deliberately have no cost,
/// no points and no effects. A fixture game therefore cannot be mistaken for a
/// test of any real card, and it can never be confused with the officially
/// verified pool (which is empty for four players at the time of writing).
pub fn fixture_4p_card_set() -> Result<CardSet, String> {
    let definition = |id: String, kind: DefinitionKind| CardDefinition {
        use_limit: None,
        id,
        kind,
        cost: vec![],
        points: 0,
        effects: vec![],
    };
    let mut definitions = Vec::with_capacity(FIXTURE_DRAFT_POOL_PER_KIND * 2);
    for index in 0..FIXTURE_DRAFT_POOL_PER_KIND {
        definitions.push(definition(
            format!("FIX-OCC-{index:02}"),
            DefinitionKind::Occupation,
        ));
        definitions.push(definition(
            format!("FIX-MIN-{index:02}"),
            DefinitionKind::MinorImprovement,
        ));
    }
    let set = CardSet { definitions };
    set.validate(4)?;
    Ok(set)
}

/// A `draft_10_7_simultaneous` setup over the fixture pool.
///
/// The ruleset is the project's already-implemented right/right pass with the
/// occupation phase first. It is **not** a new ruling on the open #155 draft
/// conventions: those stay undecided (design U1), and the manifest records the
/// open items.
pub fn fixture_draft_setup(seed: u64) -> Result<DraftSetup, String> {
    let set = fixture_4p_card_set()?;
    let occupations = set
        .definitions
        .iter()
        .filter(|card| card.kind == DefinitionKind::Occupation)
        .map(|card| card.id.clone())
        .collect::<Vec<_>>();
    let minors = set
        .definitions
        .iter()
        .filter(|card| card.kind == DefinitionKind::MinorImprovement)
        .map(|card| card.id.clone())
        .collect::<Vec<_>>();
    let setup = DraftSetup {
        mode: DraftMode::Draft10_7Simultaneous,
        ruleset: DraftRuleset::explicit(
            "eval-4p-10-7-right-v1",
            Direction::Right,
            Direction::Right,
            true,
        ),
        pool: DraftPool::new(occupations, minors),
        player_count: 4,
        seed,
        deck_version: "eval-fixture-4p-v1".into(),
    };
    setup.validate()?;
    Ok(setup)
}

/// Per-seat draft execution record (design §5.3 draft extension).
pub struct DraftRun {
    pub state: DraftState,
    /// One lifetime in nanoseconds per pick decision, keyed by seat.
    pub pick_ns_by_seat: Vec<Vec<u64>>,
    /// Illegal-pick fallbacks per seat, so a row never attributes another
    /// seat's fallback to the focus seat. This is **not** a deadline fallback:
    /// the draft AI has no enforced deadline yet.
    pub fallbacks_by_seat: Vec<u32>,
    pub replay_hash: String,
}

/// Run the draft with one observation-only policy per seat.
///
/// Every decision is timed and recorded. A policy that errors, or whose pick is
/// rejected by the draft state, is replaced by the engine's deterministic
/// `cpu_pick` fallback for that decision; the fallback is counted, never hidden.
/// The completed draft is replayed from its action log and must reproduce the
/// same state hash before the harness starts a game from it.
pub fn play_draft(setup: &DraftSetup, seat_ais: &[DraftAiKind]) -> Result<DraftRun, String> {
    if seat_ais.len() != setup.player_count {
        return Err("draft seat policy list must match the player count".into());
    }
    // This runner is only valid for the harness fixture deck. The catalog is
    // rebuilt from that deck, so accepting another setup would silently score
    // its cards against the wrong catalog.
    if setup.deck_version != "eval-fixture-4p-v1" {
        return Err("play_draft only supports the eval fixture deck".into());
    }
    let set = fixture_4p_card_set()?;
    let catalog = card_catalog(&set);
    let mut state = setup.start()?;
    let mut pick_ns_by_seat = vec![Vec::new(); setup.player_count];
    let mut fallbacks_by_seat = vec![0u32; setup.player_count];
    if state.mode == DraftMode::Deal7 {
        return Ok(DraftRun {
            replay_hash: state.log_hash(),
            state,
            pick_ns_by_seat,
            fallbacks_by_seat,
        });
    }
    let mut guard = 0;
    while !state.is_complete() {
        guard += 1;
        if guard > 128 {
            return Err("draft did not terminate".into());
        }
        let token = state.current_token();
        for (seat, &ai) in seat_ais.iter().enumerate() {
            let obs = state.observe_with_catalog(seat, token, &catalog)?;
            let started = Instant::now();
            let decision = ai.decide_pick(&obs, &mut decision_rng(setup.seed, seat, token, ai));
            let mut used_fallback = false;
            // A rejected pick is retried once through the guaranteed-legal
            // deterministic fallback, exactly like the search fallback path.
            let mut commit = match decision {
                Ok(decision) => state
                    .commit_pick(seat, token, decision.occupation, decision.minor)
                    .map_err(|error| error.to_string()),
                Err(error) => Err(error),
            };
            if commit.is_err() {
                used_fallback = true;
                let fallback = cpu_pick(&obs)?;
                state.commit_pick(seat, token, fallback.occupation, fallback.minor)?;
                commit = Ok(());
            }
            commit?;
            let ns = started.elapsed().as_nanos().min(u128::from(u64::MAX)) as u64;
            pick_ns_by_seat[seat].push(ns);
            if used_fallback {
                fallbacks_by_seat[seat] += 1;
            }
        }
    }
    for (occupations, minors) in state.final_hands() {
        if occupations.len() != DRAFT_107_HAND_PER_KIND || minors.len() != DRAFT_107_HAND_PER_KIND {
            return Err("draft finished without the 7 + 7 hand invariant".into());
        }
    }
    let replay = replay_draft(setup, &state.actions)?;
    if replay.state_hash() != state.state_hash() {
        return Err("draft replay mismatch".into());
    }
    Ok(DraftRun {
        replay_hash: state.log_hash(),
        state,
        pick_ns_by_seat,
        fallbacks_by_seat,
    })
}

/// Deterministic decision name for a seat's game policy. PUCT is identified by
/// its full configuration id so two unnamed `puct` variants can never be pooled.
pub fn game_policy_id(ai: Ai, puct: &PuctConfig) -> String {
    match ai {
        Ai::Puct => puct_config_id(puct),
        other => other.name().to_string(),
    }
}

// ---------------------------------------------------------------------------
// Condition identity and run manifest (design §5.2)
// ---------------------------------------------------------------------------

/// Canonical JSON hashing for `condition_id`: keys are sorted and values are
/// serialized without whitespace, so the same condition always yields the same
/// digest on every machine.
pub fn condition_id(parts: &BTreeMap<String, String>) -> String {
    let canonical = Value::Object(
        parts
            .iter()
            .map(|(key, value)| (key.clone(), Value::String(value.clone())))
            .collect(),
    );
    let bytes = serde_json::to_vec(&canonical).unwrap_or_default();
    crate::dataset_split::sha256_hex(&bytes)
}

/// The 4-player official gate snapshot recorded in the manifest (design U12).
/// Counts are measured from the code-side registry, never copied from docs.
pub fn pool_gate_snapshot(players: usize, measured_at_sha: Option<&str>) -> Value {
    use crate::card_pool::{
        builtin_implemented_cards, builtin_review_registry, usable_minor_improvements,
        usable_occupations,
    };
    let set = CardSet {
        definitions: builtin_implemented_cards(),
    };
    let registry = builtin_review_registry();
    let occupations = usable_occupations(&set, &registry, players);
    let minors = usable_minor_improvements(&set, &registry, players);
    json!({
        "players": players,
        "usable_occupations_verified": occupations,
        "usable_minor_improvements_verified": minors,
        "usable_occupations_4p": occupations,
        "usable_minor_improvements_4p": minors,
        "g1_required_per_kind": crate::card_pool::G1_MIN_PER_KIND,
        "g1_met": occupations >= crate::card_pool::G1_MIN_PER_KIND
            && minors >= crate::card_pool::G1_MIN_PER_KIND,
        "measured_from": "src/card_pool.rs builtin_review_registry (fail closed: absent or unverified = not usable)",
        "measured_at_sha": measured_at_sha,
    })
}

/// Versioned draft policy identity (design §2.5, §4.6, U11). The heuristic is
/// fixed for now; if it changes, this version must change with it.
pub const DRAFT_POLICY_VERSION: &str = "draft-ai-v1";

pub fn draft_policy_id(ai: DraftAiKind) -> String {
    format!("{}@{DRAFT_POLICY_VERSION}", ai.id())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dealt_per_kind_matches_the_engine() {
        assert_eq!(
            DRAFT_107_DEALT_PER_KIND,
            crate::draft::DraftMode::Draft10_7Simultaneous.dealt_per_kind()
        );
    }

    #[test]
    fn fixture_pool_has_the_four_player_10_7_supply() {
        let set = fixture_4p_card_set().unwrap();
        assert_eq!(
            set.definitions
                .iter()
                .filter(|card| card.kind == DefinitionKind::Occupation)
                .count(),
            40
        );
        assert_eq!(
            set.definitions
                .iter()
                .filter(|card| card.kind == DefinitionKind::MinorImprovement)
                .count(),
            40
        );
        // A fixture card must never look like an official card id.
        assert!(set
            .definitions
            .iter()
            .all(|card| card.id.starts_with("FIX-")));
    }

    #[test]
    fn fixture_draft_is_four_player_10_7_and_completes() {
        let setup = fixture_draft_setup(4242).unwrap();
        assert_eq!(setup.player_count, 4);
        assert_eq!(setup.mode, DraftMode::Draft10_7Simultaneous);
        let ais = [DraftAiKind::CardValue; 4];
        let run = play_draft(&setup, &ais).unwrap();
        assert!(run.state.is_complete());
        assert_eq!(run.pick_ns_by_seat.len(), 4);
        assert_eq!(run.fallbacks_by_seat, vec![0, 0, 0, 0]);
        for (occupations, minors) in run.state.final_hands() {
            assert_eq!(occupations.len(), 7);
            assert_eq!(minors.len(), 7);
        }
        assert_eq!(run.replay_hash, run.state.log_hash());
    }

    #[test]
    fn fixture_draft_is_deterministic_for_a_fixed_seed() {
        let setup = fixture_draft_setup(77).unwrap();
        let ais = [
            DraftAiKind::CardValue,
            DraftAiKind::Random,
            DraftAiKind::Synergy,
            DraftAiKind::CardValue,
        ];
        let first = play_draft(&setup, &ais).unwrap();
        let second = play_draft(&setup, &ais).unwrap();
        assert_eq!(first.state.state_hash(), second.state.state_hash());
        assert_eq!(first.state.final_hands(), second.state.final_hands());
    }

    #[test]
    fn condition_id_is_order_independent_and_changes_with_a_part() {
        let mut first = BTreeMap::new();
        first.insert("players".to_string(), "4".to_string());
        first.insert("draft_mode".to_string(), "none".to_string());
        let mut second = BTreeMap::new();
        second.insert("draft_mode".to_string(), "none".to_string());
        second.insert("players".to_string(), "4".to_string());
        assert_eq!(condition_id(&first), condition_id(&second));
        let mut third = second.clone();
        third.insert(
            "draft_mode".to_string(),
            "draft_10_7_simultaneous".to_string(),
        );
        assert_ne!(condition_id(&second), condition_id(&third));
        assert_eq!(condition_id(&first).len(), 64);
    }

    #[test]
    fn game_policy_ids_name_puct_by_its_full_configuration() {
        let default = PuctConfig::default();
        assert_eq!(game_policy_id(Ai::Eval, &default), "eval");
        assert_eq!(game_policy_id(Ai::Puct, &default), puct_config_id(&default));
        let mut changed = default;
        changed.prior_temperature = 3.0;
        assert_ne!(
            game_policy_id(Ai::Puct, &default),
            game_policy_id(Ai::Puct, &changed)
        );
    }
}

#[cfg(test)]
mod fixture_contract_tests {
    use super::*;

    #[test]
    fn fixture_cards_have_no_cost_points_or_effects() {
        let set = fixture_4p_card_set().unwrap();
        assert!(set.definitions.iter().all(|card| card.cost.is_empty()));
        assert!(set.definitions.iter().all(|card| card.points == 0));
        assert!(set.definitions.iter().all(|card| card.effects.is_empty()));
    }

    #[test]
    fn fixture_deck_setup_is_the_only_accepted_draft_deck() {
        let mut setup = fixture_draft_setup(1).unwrap();
        assert!(play_draft(&setup, &[DraftAiKind::CardValue; 4]).is_ok());
        setup.deck_version = "some-other-deck".into();
        assert!(play_draft(&setup, &[DraftAiKind::CardValue; 4]).is_err());
    }

    #[test]
    fn draft_policy_ids_carry_their_version() {
        assert_eq!(draft_policy_id(DraftAiKind::Synergy), "synergy@draft-ai-v1");
        assert_eq!(draft_policy_id(DraftAiKind::Random), "random@draft-ai-v1");
    }
}
