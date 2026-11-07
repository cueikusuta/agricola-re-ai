//! Reproducible seat-rotated AI comparisons for M8-3/M13.
//! A JSONL row is one completed game. Each seed block contains every
//! candidate seat for both comparison arms. The optional M7 MCTS arm runs
//! under a per-decision wall-clock budget and records whether it timed out or
//! fell back to the guaranteed-legal evaluation policy.
//!
//! `--cards` selects the card condition. M13 compares cardless and
//! implemented-card conditions as separate experiments, so the condition, the
//! deck size and the deck content hash are recorded on every row and the
//! aggregator refuses to pool them. Only the built-in, two-player A/B pool is
//! available; no unverified card can enter a deck here.
//!
//! `--search-mode` selects the stopping rule (#164). `simulation-capped` runs a
//! fixed number of simulations and is reproducible; `time-budgeted` searches
//! until a wall-clock deadline and is not. The two are different experiments
//! and the aggregator refuses to mix them.
#![recursion_limit = "256"]
use agricola_re_ai::cards::{verified_basic_card_set, CardSet};
use agricola_re_ai::evaluation::EvaluationPolicy;
use agricola_re_ai::harvest::SimpleHarvestDetails;
use agricola_re_ai::mcts::{MctsOutcome, MctsPolicy, SearchLimit, SearchMode};
use agricola_re_ai::model::Space;
use agricola_re_ai::policy::{
    play_to_end_observed, CardChoiceDetails, DetailPolicy, GreedyPolicy, RandomPolicy,
    SimpleDetails, SpacePolicy,
};
use agricola_re_ai::score::{standings, standings_with_cards};
use agricola_re_ai::{Config, Game, Observation};
use rand_chacha::ChaCha8Rng;
use serde_json::json;
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::fs::File;
use std::io::{BufWriter, Write};
use std::time::{Duration, Instant};

const DEFAULT_DECISION_BUDGET_MS: u64 = 10_000;
const DEFAULT_SIMULATIONS: usize = 64;
/// Safety cap for time-budgeted search. It is high enough not to bind on small
/// games, and low enough that a pathological position cannot loop forever.
const DEFAULT_TIME_BUDGET_MAX_SIMULATIONS: usize = 100_000;

/// Card condition of an experiment arm.
///
/// M13 must report cardless and implemented-card conditions separately, never
/// pooled. `verified` means the built-in A/B pool that
/// `verified_basic_card_set` already gates to two players; nothing here
/// promotes an unverified card into a deck.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Cards {
    None,
    Verified,
}

impl Cards {
    fn parse(name: &str) -> Result<Self, String> {
        match name {
            "none" => Ok(Cards::None),
            // Both spellings are accepted so the GitHub Actions input can be
            // forwarded verbatim to this CLI and to the aggregation script.
            "verified" | "verified_basic_2p" => Ok(Cards::Verified),
            _ => Err(format!("unsupported card condition: {name}")),
        }
    }
    fn name(self) -> &'static str {
        match self {
            Cards::None => "none",
            Cards::Verified => "verified_basic_2p",
        }
    }
    fn is_enabled(self) -> bool {
        self != Cards::None
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Ai {
    Random,
    Greedy,
    Eval,
    Mcts,
}

impl Ai {
    fn parse(name: &str) -> Result<Self, String> {
        match name {
            "random" => Ok(Ai::Random),
            "greedy" => Ok(Ai::Greedy),
            "eval" => Ok(Ai::Eval),
            "mcts" => Ok(Ai::Mcts),
            _ => Err(format!("unsupported AI: {name}")),
        }
    }
    fn name(self) -> &'static str {
        match self {
            Ai::Random => "random",
            Ai::Greedy => "greedy",
            Ai::Eval => "eval",
            Ai::Mcts => "mcts",
        }
    }
    fn is_mcts(self) -> bool {
        self == Ai::Mcts
    }
}

struct Options {
    players: usize,
    seed_start: u64,
    block_start: usize,
    blocks: usize,
    candidate: Ai,
    baseline: Ai,
    opponent: Ai,
    additional_action_space: bool,
    cards: Cards,
    decision_budget_ms: u64,
    search_mode: SearchMode,
    mcts_simulations: usize,
    time_budget_max_simulations: usize,
    mcts_tree_depth: usize,
    output: String,
}

fn option<'a>(values: &'a BTreeMap<String, String>, key: &str) -> Result<&'a str, String> {
    values
        .get(key)
        .map(String::as_str)
        .ok_or_else(|| format!("missing --{key}"))
}

/// The built-in verified pool is two-player only, so enabling it in a
/// three- or four-player experiment must fail before any game runs.
fn check_card_condition(cards: Cards, players: usize) -> Result<(), String> {
    if cards.is_enabled() && players != 2 {
        return Err(
            "the verified card pool currently supports two players only (see verified_basic_card_set)"
                .into(),
        );
    }
    Ok(())
}

fn optional_option<'a>(values: &'a BTreeMap<String, String>, key: &str) -> Option<&'a str> {
    values.get(key).map(String::as_str)
}

fn parse_options() -> Result<Options, String> {
    let mut values = BTreeMap::new();
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        let key = arg
            .strip_prefix("--")
            .ok_or_else(|| format!("invalid argument: {arg}"))?;
        if ![
            "players",
            "seed-start",
            "block-start",
            "blocks",
            "candidate",
            "baseline",
            "opponent",
            "additional-action-space",
            "cards",
            "decision-budget-ms",
            "search-mode",
            "time-budget-max-simulations",
            "mcts-simulations",
            "mcts-tree-depth",
            "output",
        ]
        .contains(&key)
        {
            return Err(format!("unsupported option: --{key}"));
        }
        let value = args
            .next()
            .ok_or_else(|| format!("missing value for --{key}"))?;
        if values.insert(key.to_string(), value).is_some() {
            return Err(format!("duplicate option: --{key}"));
        }
    }
    let players = option(&values, "players")?
        .parse::<usize>()
        .map_err(|_| "invalid players".to_string())?;
    let seed_start = option(&values, "seed-start")?
        .parse::<u64>()
        .map_err(|_| "invalid seed-start".to_string())?;
    let block_start = option(&values, "block-start")?
        .parse::<usize>()
        .map_err(|_| "invalid block-start".to_string())?;
    let blocks = option(&values, "blocks")?
        .parse::<usize>()
        .map_err(|_| "invalid blocks".to_string())?;
    let candidate = Ai::parse(option(&values, "candidate")?)?;
    let baseline = Ai::parse(option(&values, "baseline")?)?;
    let opponent = Ai::parse(option(&values, "opponent")?)?;
    let additional_action_space = match option(&values, "additional-action-space")? {
        "true" => true,
        "false" => false,
        _ => return Err("additional-action-space must be true or false".into()),
    };
    let cards = Cards::parse(optional_option(&values, "cards").unwrap_or("none"))?;
    let decision_budget_ms = optional_option(&values, "decision-budget-ms")
        .map(|value| {
            value
                .parse::<u64>()
                .map_err(|_| "invalid decision-budget-ms".to_string())
        })
        .transpose()?
        .unwrap_or(DEFAULT_DECISION_BUDGET_MS);
    let mcts_simulations = optional_option(&values, "mcts-simulations")
        .map(|value| {
            value
                .parse::<usize>()
                .map_err(|_| "invalid mcts-simulations".to_string())
        })
        .transpose()?
        .unwrap_or(DEFAULT_SIMULATIONS);
    let time_budget_max_simulations = optional_option(&values, "time-budget-max-simulations")
        .map(|value| {
            value
                .parse::<usize>()
                .map_err(|_| "invalid time-budget-max-simulations".to_string())
        })
        .transpose()?
        .unwrap_or(DEFAULT_TIME_BUDGET_MAX_SIMULATIONS);
    // The mode is explicit (#164). Defaulting to `simulation-capped` keeps every
    // existing invocation reproducible and comparable with past results.
    let search_mode_value = optional_option(&values, "search-mode").unwrap_or("simulation-capped");
    let search_mode = SearchMode::parse(search_mode_value)
        .ok_or_else(|| format!("unsupported search mode: {search_mode_value}"))?;
    let mcts_tree_depth = optional_option(&values, "mcts-tree-depth")
        .map(|value| {
            value
                .parse::<usize>()
                .map_err(|_| "invalid mcts-tree-depth".to_string())
        })
        .transpose()?
        .unwrap_or(2);
    let output = option(&values, "output")?.to_string();
    if !(2..=4).contains(&players) {
        return Err("players must be 2, 3, or 4".into());
    }
    if blocks == 0 || blocks > 1000 || block_start > 1_000_000 {
        return Err("invalid number or start of seed blocks".into());
    }
    if seed_start
        .checked_add(block_start as u64)
        .and_then(|s| s.checked_add(blocks as u64))
        .is_none()
    {
        return Err("seed range overflows u64".into());
    }
    if decision_budget_ms > 60_000 {
        return Err("decision-budget-ms must be 0 (no deadline) or 1..=60000".into());
    }
    if mcts_simulations == 0 || mcts_simulations > 100_000 {
        return Err("mcts-simulations must be 1..=100000".into());
    }
    if time_budget_max_simulations == 0 || time_budget_max_simulations > 1_000_000 {
        return Err("time-budget-max-simulations must be 1..=1000000".into());
    }
    if !(1..=6).contains(&mcts_tree_depth) {
        return Err("mcts-tree-depth must be 1..=6".into());
    }
    if candidate == baseline {
        return Err("candidate and baseline must be different".into());
    }
    // A time budget of 0 means "no deadline"; that combination has no meaning
    // for a mode whose whole contract is the deadline.
    if search_mode == SearchMode::TimeBudgeted && decision_budget_ms == 0 {
        return Err("search-mode time-budgeted requires --decision-budget-ms > 0".into());
    }
    check_card_condition(cards, players)?;
    Ok(Options {
        players,
        seed_start,
        block_start,
        blocks,
        candidate,
        baseline,
        opponent,
        additional_action_space,
        cards,
        decision_budget_ms,
        search_mode,
        mcts_simulations,
        time_budget_max_simulations,
        mcts_tree_depth,
        output,
    })
}

/// The per-decision stopping rule for the focus seat.
///
/// Opponent seats always use `MctsPolicy.simulations` without a deadline, so a
/// time-budgeted experiment does not quietly turn three opponents into
/// unbounded searches.
fn focus_search_rule(options: &Options) -> SearchRule {
    match options.search_mode {
        SearchMode::SimulationCapped => SearchRule::Simulations(options.mcts_simulations),
        SearchMode::TimeBudgeted => SearchRule::TimeBudget {
            budget: Duration::from_millis(options.decision_budget_ms),
            max_simulations: options.time_budget_max_simulations,
        },
    }
}

#[derive(Default)]
struct FocusOutcomes {
    timeouts: u32,
    fallbacks: u32,
    simulations: u64,
    /// Largest deadline overshoot reported by the search itself.
    overshoot_ms: u64,
    /// Largest search-internal decision time reported by the search itself.
    elapsed_ms: u64,
}

struct SeatSelector<'a> {
    focus: usize,
    focus_ai: Ai,
    opponent_ai: Ai,
    random: &'a RandomPolicy,
    greedy: &'a GreedyPolicy,
    eval: &'a EvaluationPolicy,
    mcts: &'a MctsPolicy,
    /// Stopping rule for a search policy. `None` for heuristic policies and for
    /// the opponent seats, which always run the deterministic default.
    search: Option<SearchRule>,
    decision_ns: RefCell<Vec<u64>>,
    focus_outcomes: RefCell<FocusOutcomes>,
}

/// Per-decision stopping rule template. The absolute deadline is resolved at
/// the moment the decision starts.
#[derive(Clone, Copy, Debug)]
enum SearchRule {
    Simulations(usize),
    TimeBudget {
        budget: Duration,
        max_simulations: usize,
    },
}

impl SearchRule {
    fn resolve(self) -> SearchLimit {
        match self {
            SearchRule::Simulations(n) => SearchLimit::Simulations(n),
            SearchRule::TimeBudget {
                budget,
                max_simulations,
            } => SearchLimit::TimeBudget {
                deadline: Instant::now() + budget,
                max_simulations,
            },
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn select_ai(
    ai: Ai,
    random: &RandomPolicy,
    greedy: &GreedyPolicy,
    eval: &EvaluationPolicy,
    mcts: &MctsPolicy,
    obs: &Observation,
    legal: &[Space],
    rng: &mut ChaCha8Rng,
    search: Option<SearchRule>,
) -> (Option<Space>, MctsOutcome) {
    match ai {
        Ai::Random => (random.select(obs, legal, rng), MctsOutcome::default()),
        Ai::Greedy => (greedy.select(obs, legal, rng), MctsOutcome::default()),
        Ai::Eval => (eval.select(obs, legal, rng), MctsOutcome::default()),
        Ai::Mcts => {
            let limit = search
                .map(SearchRule::resolve)
                .unwrap_or(SearchLimit::Simulations(mcts.simulations));
            match mcts.select_with_limit(obs, legal, rng, limit) {
                Some((space, outcome)) => (Some(space), outcome),
                None => (None, MctsOutcome::default()),
            }
        }
    }
}

impl SpacePolicy for SeatSelector<'_> {
    fn select(&self, obs: &Observation, legal: &[Space], rng: &mut ChaCha8Rng) -> Option<Space> {
        let is_focus = obs.player_id == self.focus;
        let search = if is_focus { self.search } else { None };
        let before = Instant::now();
        let (selected, outcome) = if is_focus {
            select_ai(
                self.focus_ai,
                self.random,
                self.greedy,
                self.eval,
                self.mcts,
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
                self.mcts,
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
            if self.focus_ai.is_mcts() {
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

fn percentile_ms(values: &[u64], q: f64) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    let mut sorted = values.to_vec();
    sorted.sort_unstable();
    let rank = ((sorted.len() - 1) as f64 * q).ceil() as usize;
    sorted[rank.min(sorted.len() - 1)] as f64 / 1_000_000.0
}

fn first_credit(ranks: &[usize], seat: usize) -> Result<f64, String> {
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

fn run(options: &Options) -> Result<(), String> {
    let file = File::create(&options.output).map_err(|error| error.to_string())?;
    let mut writer = BufWriter::new(file);
    let random = RandomPolicy;
    let greedy = GreedyPolicy;
    let eval = EvaluationPolicy;
    let mcts = MctsPolicy {
        simulations: options.mcts_simulations,
        tree_depth: options.mcts_tree_depth,
    };
    // A wall-clock deadline is only enforced in time-budgeted mode. In
    // simulation-capped mode the whole point is that the search length, not the
    // clock, decides, so the run stays reproducible on any machine.
    let deadline_enforced = options.search_mode == SearchMode::TimeBudgeted;
    let search_rule = focus_search_rule(options);
    // The deck is built once and validated by the same gate the game uses, so
    // every arm and seat of an experiment provably shares one pool.
    let card_set = match options.cards {
        Cards::None => CardSet::default(),
        Cards::Verified => verified_basic_card_set(options.players)?,
    };
    let card_set_hash = card_set.content_hash();
    let card_set_count = card_set.definitions.len();
    let card_details = CardChoiceDetails;
    let details: &dyn DetailPolicy = if card_set.enabled() {
        &card_details
    } else {
        &SimpleDetails
    };
    let start = Instant::now();
    for relative_block in 0..options.blocks {
        let block_index = options.block_start + relative_block;
        let seed = options.seed_start + block_index as u64;
        for (arm_id, focus_ai) in [
            ("candidate", options.candidate),
            ("baseline", options.baseline),
        ] {
            for seat in 0..options.players {
                let config = Config {
                    players: options.players,
                    seed,
                    additional_action_space_enabled: options.additional_action_space,
                };
                let mut game = Game::with_cards(config.clone(), card_set.clone())
                    .map_err(|error| format!("seed {seed}: {error}"))?;
                let selector = SeatSelector {
                    focus: seat,
                    focus_ai,
                    opponent_ai: options.opponent,
                    random: &random,
                    greedy: &greedy,
                    eval: &eval,
                    mcts: &mcts,
                    search: Some(search_rule),
                    decision_ns: RefCell::new(Vec::new()),
                    focus_outcomes: RefCell::new(FocusOutcomes::default()),
                };
                let game_start = Instant::now();
                let focus_whole_ns = RefCell::new(Vec::<u64>::new());
                play_to_end_observed(
                    &mut game,
                    &selector,
                    details,
                    &SimpleHarvestDetails,
                    |player, ns| {
                        if player == seat {
                            focus_whole_ns.borrow_mut().push(ns);
                        }
                    },
                )
                .map_err(|error| format!("seed {seed} arm {arm_id} seat {seat}: {error}"))?;
                if !game.public.finished {
                    return Err(format!("seed {seed}: game did not finish"));
                }
                let summary = if card_set.enabled() {
                    standings_with_cards(&game.public.players, &game.card_set)
                } else {
                    standings(&game.public.players)
                };
                let hash = game.state_hash();
                let replay = Game::replay_with_cards(config, card_set.clone(), &game.log.events)
                    .map_err(|error| {
                        format!("seed {seed} arm {arm_id} seat {seat} replay: {error}")
                    })?;
                if replay.state_hash() != hash {
                    return Err(format!(
                        "seed {seed} arm {arm_id} seat {seat}: replay mismatch"
                    ));
                }
                let scores = summary
                    .scores
                    .iter()
                    .map(|score| score.total)
                    .collect::<Vec<_>>();
                let own_score = scores[seat];
                let others_mean =
                    (scores.iter().sum::<i32>() - own_score) as f64 / (options.players - 1) as f64;
                let decision_ns = selector.decision_ns.borrow();
                let outcomes = selector.focus_outcomes.borrow();
                let whole_ns = focus_whole_ns.borrow();
                let budget_ns = options.decision_budget_ms * 1_000_000;
                let focus_timeout_count = if deadline_enforced && focus_ai.is_mcts() {
                    whole_ns.iter().filter(|&&ns| ns > budget_ns).count()
                } else {
                    0
                };
                let focus_max_decision_ms =
                    whole_ns.iter().copied().max().unwrap_or(0) as f64 / 1_000_000.0;
                let record = json!({
                    "schema_version": 1,
                    "block_index": block_index,
                    "seed": seed,
                    "arm_id": arm_id,
                    "focus_policy": focus_ai.name(),
                    "opponent_policy": options.opponent.name(),
                    "candidate_seat": seat,
                    "players": options.players,
                    "additional_action_space": options.additional_action_space,
                    "cards": options.cards.name(),
                    "card_set_count": card_set_count,
                    "card_set_hash": card_set_hash,
                    "finished": true,
                    "rounds_completed": 14,
                    "scores_by_seat": scores,
                    "ranks_by_seat": summary.rank,
                    "first_place_credit": first_credit(&summary.rank, seat)?,
                    "expected_rank_observation": summary.rank[seat],
                    "score_margin": own_score as f64 - others_mean,
                    "decisions": decision_ns.len(),
                    "focus_decisions": whole_ns.len(),
                    "decision_budget_ms": options.decision_budget_ms,
                    "search_mode": options.search_mode.as_str(),
                    "mcts_simulations_cap": options.mcts_simulations,
                    "time_budget_max_simulations": options.time_budget_max_simulations,
                    "search_budget_enforced": deadline_enforced,
                    "mcts_deadline_active": focus_ai.is_mcts() && deadline_enforced,
                    "deterministic_policy_selection": !(focus_ai.is_mcts() && deadline_enforced),
                    "max_policy_select_ms": decision_ns.iter().copied().max().unwrap_or(0) as f64 / 1_000_000.0,
                    "p50_policy_select_ms": percentile_ms(&decision_ns, 0.50),
                    "p95_policy_select_ms": percentile_ms(&decision_ns, 0.95),
                    "p99_policy_select_ms": percentile_ms(&decision_ns, 0.99),
                    "focus_max_decision_ms": focus_max_decision_ms,
                    "focus_p95_decision_ms": percentile_ms(&whole_ns, 0.95),
                    "focus_p99_decision_ms": percentile_ms(&whole_ns, 0.99),
                    "focus_timeout_count": focus_timeout_count,
                    "timeout_count": outcomes.timeouts,
                    "fallback_count": outcomes.fallbacks,
                    "mcts_simulations_completed": outcomes.simulations,
                    "mcts_simulations_per_decision": if whole_ns.is_empty() {
                        0.0
                    } else {
                        outcomes.simulations as f64 / whole_ns.len() as f64
                    },
                    "max_deadline_overshoot_ms": outcomes.overshoot_ms,
                    "max_search_elapsed_ms": outcomes.elapsed_ms,
                    "mcts_tree_depth": options.mcts_tree_depth,
                    "policy_select_only": true,
                    "total_wall_ms": game_start.elapsed().as_secs_f64() * 1000.0,
                    "replay_hash": hash,
                    "replay_verified": true,
                });
                serde_json::to_writer(&mut writer, &record).map_err(|error| error.to_string())?;
                writer.write_all(b"\n").map_err(|error| error.to_string())?;
            }
        }
    }
    writer.flush().map_err(|error| error.to_string())?;
    eprintln!(
        "completed {} games in {:.3}s; output={}",
        options.blocks * options.players * 2,
        start.elapsed().as_secs_f64(),
        options.output
    );
    Ok(())
}

fn main() {
    if let Err(error) = parse_options().and_then(|options| run(&options)) {
        eprintln!("paired evaluation failed: {error}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shared_first_credit_sums_to_one() {
        let ranks = [1, 1, 3, 4];
        assert_eq!(first_credit(&ranks, 0).unwrap(), 0.5);
        assert_eq!(first_credit(&ranks, 1).unwrap(), 0.5);
        assert_eq!(first_credit(&ranks, 2).unwrap(), 0.0);
    }

    #[test]
    fn rejects_invalid_ranks() {
        assert!(first_credit(&[2, 3], 0).is_err());
        assert!(first_credit(&[1, 2], 3).is_err());
    }

    #[test]
    fn percentiles_are_nearest_rank_and_bounded() {
        let values = [1_000_000, 2_000_000, 3_000_000, 4_000_000];
        assert_eq!(percentile_ms(&values, 0.0), 1.0);
        assert_eq!(percentile_ms(&values, 0.99), 4.0);
        assert_eq!(percentile_ms(&[], 0.95), 0.0);
    }

    #[test]
    fn mcts_is_a_recognized_ai_name() {
        assert_eq!(Ai::parse("mcts").unwrap(), Ai::Mcts);
        assert!(Ai::parse("stockfish").is_err());
    }

    #[test]
    fn card_condition_parses_and_names_itself() {
        assert_eq!(Cards::parse("none").unwrap(), Cards::None);
        assert_eq!(Cards::parse("verified").unwrap(), Cards::Verified);
        assert_eq!(
            Cards::parse("verified_basic_2p").unwrap(),
            Cards::Verified,
            "the workflow forwards its own input value verbatim"
        );
        assert!(Cards::parse("all_840").is_err());
        assert_eq!(Cards::None.name(), "none");
        assert_eq!(Cards::Verified.name(), "verified_basic_2p");
        assert!(!Cards::None.is_enabled());
        assert!(Cards::Verified.is_enabled());
    }

    #[test]
    fn cardless_constructor_retains_legacy_game_hash() {
        let config = Config {
            players: 2,
            seed: 42,
            additional_action_space_enabled: false,
        };
        let legacy = Game::new(config.clone());
        let explicit = Game::with_cards(config, CardSet::default()).unwrap();
        assert_eq!(legacy.state_hash(), explicit.state_hash());
    }

    #[test]
    fn verified_cards_are_rejected_outside_two_players() {
        for players in [2, 3, 4] {
            assert!(check_card_condition(Cards::None, players).is_ok());
        }
        assert!(check_card_condition(Cards::Verified, 2).is_ok());
        for players in [3, 4] {
            assert!(
                check_card_condition(Cards::Verified, players).is_err(),
                "an unverifiable pool must fail closed at {players} players"
            );
        }
    }
}
