//! Reproducible seat-rotated, cardless AI comparisons for M8-3.
//! A JSONL row is one completed game. Each seed block contains every
//! candidate seat for both comparison arms. The optional M7 MCTS arm runs
//! under a per-decision wall-clock budget and records whether it timed out or
//! fell back to the guaranteed-legal evaluation policy.
#![recursion_limit = "256"]
use agricola_re_ai::evaluation::EvaluationPolicy;
use agricola_re_ai::harvest::SimpleHarvestDetails;
use agricola_re_ai::mcts::{MctsOutcome, MctsPolicy};
use agricola_re_ai::mcts_puct::{LeafMode, PuctConfig, PuctPolicy};
use agricola_re_ai::model::Space;
use agricola_re_ai::policy::{
    play_to_end_observed, GreedyPolicy, RandomPolicy, SimpleDetails, SpacePolicy,
};
use agricola_re_ai::score::standings;
use agricola_re_ai::{Config, Game, Observation};
use rand_chacha::ChaCha8Rng;
use serde_json::json;
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::fs::File;
use std::io::{BufWriter, Write};
use std::time::{Duration, Instant};

const DEFAULT_DECISION_BUDGET_MS: u64 = 10_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Ai {
    Random,
    Greedy,
    Eval,
    Mcts,
    Puct,
}

impl Ai {
    fn parse(name: &str) -> Result<Self, String> {
        match name {
            "random" => Ok(Ai::Random),
            "greedy" => Ok(Ai::Greedy),
            "eval" => Ok(Ai::Eval),
            "mcts" => Ok(Ai::Mcts),
            "puct" => Ok(Ai::Puct),
            _ => Err(format!("unsupported AI: {name}")),
        }
    }
    fn name(self) -> &'static str {
        match self {
            Ai::Random => "random",
            Ai::Greedy => "greedy",
            Ai::Eval => "eval",
            Ai::Mcts => "mcts",
            Ai::Puct => "puct",
        }
    }
    /// Search policies use a per-decision wall-clock budget and a fallback.
    fn is_search(self) -> bool {
        matches!(self, Ai::Mcts | Ai::Puct)
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
    decision_budget_ms: u64,
    mcts_simulations: usize,
    mcts_tree_depth: usize,
    puct_simulations: usize,
    puct_tree_depth: usize,
    puct_c: f64,
    puct_prior_temperature: f64,
    puct_value_temperature: f64,
    puct_leaf: LeafMode,
    puct_root_limit: usize,
    puct_final_prior: f64,
    output: String,
}

fn option<'a>(values: &'a BTreeMap<String, String>, key: &str) -> Result<&'a str, String> {
    values
        .get(key)
        .map(String::as_str)
        .ok_or_else(|| format!("missing --{key}"))
}

fn optional_option<'a>(values: &'a BTreeMap<String, String>, key: &str) -> Option<&'a str> {
    values.get(key).map(String::as_str)
}

fn parse_option_or<T>(
    values: &BTreeMap<String, String>,
    key: &str,
    default: T,
    error: &str,
) -> Result<T, String>
where
    T: std::str::FromStr,
{
    match optional_option(values, key) {
        Some(value) => value.parse::<T>().map_err(|_| error.to_string()),
        None => Ok(default),
    }
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
            "decision-budget-ms",
            "mcts-simulations",
            "mcts-tree-depth",
            "puct-simulations",
            "puct-tree-depth",
            "puct-c",
            "puct-prior-temp",
            "puct-value-temp",
            "puct-leaf",
            "puct-root-limit",
            "puct-final-prior",
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
        .unwrap_or(64);
    let mcts_tree_depth = optional_option(&values, "mcts-tree-depth")
        .map(|value| {
            value
                .parse::<usize>()
                .map_err(|_| "invalid mcts-tree-depth".to_string())
        })
        .transpose()?
        .unwrap_or(2);
    let default_puct = PuctConfig::default();
    let puct_simulations = parse_option_or(
        &values,
        "puct-simulations",
        default_puct.simulations,
        "invalid puct-simulations",
    )?;
    let puct_tree_depth = parse_option_or(
        &values,
        "puct-tree-depth",
        default_puct.tree_depth,
        "invalid puct-tree-depth",
    )?;
    let puct_c = parse_option_or(&values, "puct-c", default_puct.c_puct, "invalid puct-c")?;
    let puct_prior_temperature = parse_option_or(
        &values,
        "puct-prior-temp",
        default_puct.prior_temperature,
        "invalid puct-prior-temp",
    )?;
    let puct_value_temperature = parse_option_or(
        &values,
        "puct-value-temp",
        default_puct.value_temperature,
        "invalid puct-value-temp",
    )?;
    let puct_leaf = match optional_option(&values, "puct-leaf").unwrap_or("heuristic") {
        "heuristic" => LeafMode::Heuristic,
        "terminal" => LeafMode::Terminal,
        _ => return Err("puct-leaf must be heuristic or terminal".into()),
    };
    let puct_root_limit = parse_option_or(
        &values,
        "puct-root-limit",
        default_puct.root_candidate_limit,
        "invalid puct-root-limit",
    )?;
    let puct_final_prior = parse_option_or(
        &values,
        "puct-final-prior",
        default_puct.final_prior_weight,
        "invalid puct-final-prior",
    )?;
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
    if !(1..=6).contains(&mcts_tree_depth) {
        return Err("mcts-tree-depth must be 1..=6".into());
    }
    if puct_simulations == 0 || puct_simulations > 100_000 {
        return Err("puct-simulations must be 1..=100000".into());
    }
    if !(1..=6).contains(&puct_tree_depth) {
        return Err("puct-tree-depth must be 1..=6".into());
    }
    if !puct_c.is_finite() || puct_c < 0.0 || puct_c > 100.0 {
        return Err("puct-c must be finite and 0..=100".into());
    }
    if !puct_prior_temperature.is_finite() || puct_prior_temperature <= 0.0 {
        return Err("puct-prior-temp must be finite and > 0".into());
    }
    if !puct_value_temperature.is_finite() || puct_value_temperature <= 0.0 {
        return Err("puct-value-temp must be finite and > 0".into());
    }
    if puct_root_limit > 100 {
        return Err("puct-root-limit must be 0..=100".into());
    }
    if !puct_final_prior.is_finite() || !(0.0..=10.0).contains(&puct_final_prior) {
        return Err("puct-final-prior must be finite and 0..=10".into());
    }
    if candidate == baseline {
        return Err("candidate and baseline must be different".into());
    }
    Ok(Options {
        players,
        seed_start,
        block_start,
        blocks,
        candidate,
        baseline,
        opponent,
        additional_action_space,
        decision_budget_ms,
        mcts_simulations,
        mcts_tree_depth,
        puct_simulations,
        puct_tree_depth,
        puct_c,
        puct_prior_temperature,
        puct_value_temperature,
        puct_leaf,
        puct_root_limit,
        puct_final_prior,
        output,
    })
}

#[derive(Default)]
struct FocusOutcomes {
    timeouts: u32,
    fallbacks: u32,
    simulations: u64,
}

struct SeatSelector<'a> {
    focus: usize,
    focus_ai: Ai,
    opponent_ai: Ai,
    random: &'a RandomPolicy,
    greedy: &'a GreedyPolicy,
    eval: &'a EvaluationPolicy,
    mcts: &'a MctsPolicy,
    puct: &'a PuctPolicy,
    budget: Option<Duration>,
    decision_ns: RefCell<Vec<u64>>,
    focus_outcomes: RefCell<FocusOutcomes>,
}

#[allow(clippy::too_many_arguments)]
fn select_ai(
    ai: Ai,
    random: &RandomPolicy,
    greedy: &GreedyPolicy,
    eval: &EvaluationPolicy,
    mcts: &MctsPolicy,
    puct: &PuctPolicy,
    obs: &Observation,
    legal: &[Space],
    rng: &mut ChaCha8Rng,
    deadline: Option<Instant>,
) -> (Option<Space>, MctsOutcome) {
    match ai {
        Ai::Random => (random.select(obs, legal, rng), MctsOutcome::default()),
        Ai::Greedy => (greedy.select(obs, legal, rng), MctsOutcome::default()),
        Ai::Eval => (eval.select(obs, legal, rng), MctsOutcome::default()),
        Ai::Mcts => match mcts.select_with_budget(obs, legal, rng, deadline) {
            Some((space, outcome)) => (Some(space), outcome),
            None => (None, MctsOutcome::default()),
        },
        Ai::Puct => match puct.select_with_budget(obs, legal, rng, deadline) {
            Some((space, outcome)) => (Some(space), outcome),
            None => (None, MctsOutcome::default()),
        },
    }
}

impl SpacePolicy for SeatSelector<'_> {
    fn select(&self, obs: &Observation, legal: &[Space], rng: &mut ChaCha8Rng) -> Option<Space> {
        let is_focus = obs.player_id == self.focus;
        let deadline = if is_focus {
            self.budget.map(|budget| Instant::now() + budget)
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
                self.mcts,
                self.puct,
                obs,
                legal,
                rng,
                deadline,
            )
        } else {
            select_ai(
                self.opponent_ai,
                self.random,
                self.greedy,
                self.eval,
                self.mcts,
                self.puct,
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
                let over_budget = self
                    .budget
                    .is_some_and(|budget| ns > budget.as_nanos() as u64);
                if outcome.timed_out || over_budget {
                    outcomes.timeouts += 1;
                }
                if outcome.used_fallback {
                    outcomes.fallbacks += 1;
                }
                outcomes.simulations += outcome.simulations_completed as u64;
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
    let puct = PuctPolicy {
        config: PuctConfig {
            simulations: options.puct_simulations,
            tree_depth: options.puct_tree_depth,
            c_puct: options.puct_c,
            prior_temperature: options.puct_prior_temperature,
            value_temperature: options.puct_value_temperature,
            leaf_mode: options.puct_leaf,
            root_candidate_limit: options.puct_root_limit,
            final_prior_weight: options.puct_final_prior,
        },
    };
    let puct_leaf_name = match options.puct_leaf {
        LeafMode::Heuristic => "heuristic",
        LeafMode::Terminal => "terminal",
    };
    let budget = Duration::from_millis(options.decision_budget_ms);
    let deadline_enabled = options.decision_budget_ms > 0;
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
                let mut game = Game::new(config.clone());
                let selector = SeatSelector {
                    focus: seat,
                    focus_ai,
                    opponent_ai: options.opponent,
                    random: &random,
                    greedy: &greedy,
                    eval: &eval,
                    mcts: &mcts,
                    puct: &puct,
                    budget: (focus_ai.is_search() && deadline_enabled).then_some(budget),
                    decision_ns: RefCell::new(Vec::new()),
                    focus_outcomes: RefCell::new(FocusOutcomes::default()),
                };
                let game_start = Instant::now();
                let focus_whole_ns = RefCell::new(Vec::<u64>::new());
                play_to_end_observed(
                    &mut game,
                    &selector,
                    &SimpleDetails,
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
                let summary = standings(&game.public.players);
                let hash = game.state_hash();
                let replay = Game::replay(config, &game.log.events).map_err(|error| {
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
                let focus_timeout_count = if deadline_enabled {
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
                    "cards": "none",
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
                    "mcts_deadline_active": focus_ai.is_search() && deadline_enabled,
                    "search_deadline_active": focus_ai.is_search() && deadline_enabled,
                    "deterministic_policy_selection": !(focus_ai.is_search() && deadline_enabled),
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
                    "search_simulations_completed": outcomes.simulations,
                    "mcts_tree_depth": options.mcts_tree_depth,
                    "puct_leaf": puct_leaf_name,
                    "puct_simulations_config": options.puct_simulations,
                    "puct_tree_depth": options.puct_tree_depth,
                    "puct_c": options.puct_c,
                    "puct_prior_temperature": options.puct_prior_temperature,
                    "puct_value_temperature": options.puct_value_temperature,
                    "puct_root_limit": options.puct_root_limit,
                    "puct_final_prior_weight": options.puct_final_prior,
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
        assert_eq!(Ai::parse("puct").unwrap(), Ai::Puct);
        assert!(Ai::Mcts.is_search() && Ai::Puct.is_search());
        assert!(!Ai::Eval.is_search() && !Ai::Random.is_search());
        assert!(Ai::parse("stockfish").is_err());
    }
}
