//! Reproducible seat-rotated AI comparisons for M8-3/M13.
//! A JSONL row is one completed game. Each seed block contains every
//! candidate seat for both comparison arms. The optional M7 MCTS arm runs
//! under a per-decision wall-clock budget and records whether it timed out or
//! fell back to the guaranteed-legal evaluation policy.
//!
//! #167 A: in `time-budgeted` mode the budget bounds the **whole decision**
//! (search plus detail generation), not only the search. The search gets the
//! budget minus a detail reserve, details get the remainder, and a decision that
//! runs out of budget falls back to the guaranteed-legal automatic details. The
//! row records whole-decision percentiles, overshoot and fallback counts so a
//! "10 s is observed, not guaranteed" claim can be checked from the artifacts.
//!
//! `--cards` selects the card condition. M13 compares cardless and
//! implemented-card conditions as separate experiments, so the condition, the
//! deck size and the deck content hash are recorded on every row and the
//! aggregator refuses to pool them. Only the built-in, two-player A/B pool is
//! available; no unverified card can enter a deck here.
#![recursion_limit = "256"]
use agricola_re_ai::cards::{verified_basic_card_set, CardSet};
use agricola_re_ai::decision_budget::{percentile_ms, DecisionBudgetSpec, DecisionTimings};
use agricola_re_ai::evaluation::{EvaluationPolicy, EvaluationVariant, VariantEvaluationPolicy};
use agricola_re_ai::harvest::SimpleHarvestDetails;
use agricola_re_ai::mcts::{MctsOutcome, MctsPolicy, SearchLimit, SearchMode};
use agricola_re_ai::mcts_puct::{LeafMode, PuctConfig, PuctPolicy};
use agricola_re_ai::model::Space;
use agricola_re_ai::policy::{
    play_to_end_with_budget, BudgetedDecisions, CardChoiceDetails, DetailPolicy, GreedyPolicy,
    RandomPolicy, SimpleDetails, SpacePolicy,
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
/// Safety cap for one time-budgeted decision. High enough not to bind on small
/// games, low enough that a pathological position cannot loop forever.
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
    EvalVeg,
    EvalFood,
    EvalVegConditional,
    EvalRoom,
    Mcts,
    Puct,
}

impl Ai {
    fn parse(name: &str) -> Result<Self, String> {
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
    fn name(self) -> &'static str {
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
    cards: Cards,
    decision_budget_ms: u64,
    /// Search window minus this is the search deadline; details get the rest.
    /// `None` derives it from the budget (#167 A).
    detail_reserve_ms: Option<u64>,
    search_mode: SearchMode,
    mcts_simulations: usize,
    time_budget_max_simulations: usize,
    mcts_tree_depth: usize,
    puct_simulations: usize,
    puct_tree_depth: usize,
    puct_c: f64,
    puct_prior_temperature: f64,
    puct_value_temperature: f64,
    puct_leaf: LeafMode,
    puct_root_limit: usize,
    puct_final_prior: f64,
    /// Configuration of the PUCT used by the **baseline arm's focus seat and
    /// every opponent PUCT seat** (#219). Unset flags default to the candidate
    /// PUCT configuration, so every pre-#219 command keeps its behaviour.
    baseline_puct_simulations: usize,
    baseline_puct_tree_depth: usize,
    baseline_puct_c: f64,
    baseline_puct_prior_temperature: f64,
    baseline_puct_value_temperature: f64,
    baseline_puct_leaf: LeafMode,
    baseline_puct_root_limit: usize,
    baseline_puct_final_prior: f64,
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
    parse_options_from(std::env::args().skip(1))
}

fn parse_options_from(args: impl Iterator<Item = String>) -> Result<Options, String> {
    let mut values = BTreeMap::new();
    let mut args = args;
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
            "detail-reserve-ms",
            "search-mode",
            "time-budget-max-simulations",
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
            "baseline-puct-simulations",
            "baseline-puct-tree-depth",
            "baseline-puct-c",
            "baseline-puct-prior-temp",
            "baseline-puct-value-temp",
            "baseline-puct-leaf",
            "baseline-puct-root-limit",
            "baseline-puct-final-prior",
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
    let detail_reserve_ms = optional_option(&values, "detail-reserve-ms")
        .map(|value| {
            value
                .parse::<u64>()
                .map_err(|_| "invalid detail-reserve-ms".to_string())
        })
        .transpose()?;
    let mcts_simulations = optional_option(&values, "mcts-simulations")
        .map(|value| {
            value
                .parse::<usize>()
                .map_err(|_| "invalid mcts-simulations".to_string())
        })
        .transpose()?
        .unwrap_or(DEFAULT_SIMULATIONS);
    let time_budget_max_simulations = parse_option_or(
        &values,
        "time-budget-max-simulations",
        DEFAULT_TIME_BUDGET_MAX_SIMULATIONS,
        "invalid time-budget-max-simulations",
    )?;
    // The stopping rule is explicit (#164). The default keeps every existing
    // invocation reproducible and comparable with past results.
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
    let puct_leaf = match optional_option(&values, "puct-leaf").unwrap_or("terminal") {
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
    // #219: the baseline/opponent PUCT configuration. If no baseline flag is
    // given the baseline is bit-for-bit the candidate configuration (the
    // pre-#219 behaviour). Pinning even one flag freezes the baseline
    // configuration, and because a silent default could silently follow the
    // candidate variant, freezing requires **all** flags explicitly: a
    // partially frozen baseline is rejected before any game runs.
    let baseline_flags_provided = [
        "baseline-puct-simulations",
        "baseline-puct-tree-depth",
        "baseline-puct-c",
        "baseline-puct-prior-temp",
        "baseline-puct-value-temp",
        "baseline-puct-leaf",
        "baseline-puct-root-limit",
        "baseline-puct-final-prior",
    ]
    .iter()
    .any(|key| values.contains_key(*key));
    let require_baseline_value = |key: &str| -> Result<&str, String> {
        optional_option(&values, key).ok_or_else(|| {
            format!(
                "freezing the baseline PUCT configuration requires all eight --baseline-puct-* flags; missing --{key}"
            )
        })
    };
    let baseline_puct_simulations = if baseline_flags_provided {
        require_baseline_value("baseline-puct-simulations")?
            .parse::<usize>()
            .map_err(|_| "invalid baseline-puct-simulations".to_string())?
    } else {
        puct_simulations
    };
    let baseline_puct_tree_depth = if baseline_flags_provided {
        require_baseline_value("baseline-puct-tree-depth")?
            .parse::<usize>()
            .map_err(|_| "invalid baseline-puct-tree-depth".to_string())?
    } else {
        puct_tree_depth
    };
    let baseline_puct_c = if baseline_flags_provided {
        require_baseline_value("baseline-puct-c")?
            .parse::<f64>()
            .map_err(|_| "invalid baseline-puct-c".to_string())?
    } else {
        puct_c
    };
    let baseline_puct_prior_temperature = if baseline_flags_provided {
        require_baseline_value("baseline-puct-prior-temp")?
            .parse::<f64>()
            .map_err(|_| "invalid baseline-puct-prior-temp".to_string())?
    } else {
        puct_prior_temperature
    };
    let baseline_puct_value_temperature = if baseline_flags_provided {
        require_baseline_value("baseline-puct-value-temp")?
            .parse::<f64>()
            .map_err(|_| "invalid baseline-puct-value-temp".to_string())?
    } else {
        puct_value_temperature
    };
    let baseline_puct_leaf = match optional_option(&values, "baseline-puct-leaf") {
        Some("heuristic") => LeafMode::Heuristic,
        Some("terminal") => LeafMode::Terminal,
        Some(_) => return Err("baseline-puct-leaf must be heuristic or terminal".into()),
        None if baseline_flags_provided => {
            return Err(
                "freezing the baseline PUCT configuration requires all eight --baseline-puct-* flags; missing --baseline-puct-leaf"
                    .into(),
            )
        }
        None => puct_leaf,
    };
    let baseline_puct_root_limit = if baseline_flags_provided {
        require_baseline_value("baseline-puct-root-limit")?
            .parse::<usize>()
            .map_err(|_| "invalid baseline-puct-root-limit".to_string())?
    } else {
        puct_root_limit
    };
    let baseline_puct_final_prior = if baseline_flags_provided {
        require_baseline_value("baseline-puct-final-prior")?
            .parse::<f64>()
            .map_err(|_| "invalid baseline-puct-final-prior".to_string())?
    } else {
        puct_final_prior
    };
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
    if baseline_puct_simulations == 0 || baseline_puct_simulations > 100_000 {
        return Err("baseline-puct-simulations must be 1..=100000".into());
    }
    if !(1..=6).contains(&baseline_puct_tree_depth) {
        return Err("baseline-puct-tree-depth must be 1..=6".into());
    }
    if !baseline_puct_c.is_finite() || baseline_puct_c < 0.0 || baseline_puct_c > 100.0 {
        return Err("baseline-puct-c must be finite and 0..=100".into());
    }
    if !baseline_puct_prior_temperature.is_finite() || baseline_puct_prior_temperature <= 0.0 {
        return Err("baseline-puct-prior-temp must be finite and > 0".into());
    }
    if !baseline_puct_value_temperature.is_finite() || baseline_puct_value_temperature <= 0.0 {
        return Err("baseline-puct-value-temp must be finite and > 0".into());
    }
    if baseline_puct_root_limit > 100 {
        return Err("baseline-puct-root-limit must be 0..=100".into());
    }
    if !baseline_puct_final_prior.is_finite() || !(0.0..=10.0).contains(&baseline_puct_final_prior)
    {
        return Err("baseline-puct-final-prior must be finite and 0..=10".into());
    }
    // Same policy names are only allowed when the two PUCT configurations
    // provably differ (#219): the whole point of a PUCT-vs-PUCT ablation is a
    // candidate variant against a frozen baseline configuration, and the
    // baseline arm stays a mirror control because its focus seat plays the
    // baseline configuration against baseline opponents.
    let baseline_puct_equals_candidate = baseline_puct_simulations == puct_simulations
        && baseline_puct_tree_depth == puct_tree_depth
        && baseline_puct_c == puct_c
        && baseline_puct_prior_temperature == puct_prior_temperature
        && baseline_puct_value_temperature == puct_value_temperature
        && baseline_puct_leaf == puct_leaf
        && baseline_puct_root_limit == puct_root_limit
        && baseline_puct_final_prior == puct_final_prior;
    if candidate == baseline && (candidate != Ai::Puct || baseline_puct_equals_candidate) {
        return Err(
            "candidate and baseline must be different; a PUCT-vs-PUCT run needs an explicit --baseline-puct-* override".into(),
        );
    }
    // #219 review P2: in time-budgeted mode the focus stopping rule ignores
    // per-policy simulation counts, so a PUCT-vs-PUCT run whose two
    // configurations differ *only* in simulations would measure nothing at
    // the focus seat. Reject it instead of reporting a silent null result.
    let baseline_puct_differs_only_in_simulations = !baseline_puct_equals_candidate
        && baseline_puct_tree_depth == puct_tree_depth
        && baseline_puct_c == puct_c
        && baseline_puct_prior_temperature == puct_prior_temperature
        && baseline_puct_value_temperature == puct_value_temperature
        && baseline_puct_leaf == puct_leaf
        && baseline_puct_root_limit == puct_root_limit
        && baseline_puct_final_prior == puct_final_prior
        && baseline_puct_simulations != puct_simulations;
    if search_mode == SearchMode::TimeBudgeted
        && candidate == Ai::Puct
        && candidate == baseline
        && baseline_puct_differs_only_in_simulations
    {
        return Err(
            "a time-budgeted PUCT-vs-PUCT run must differ in more than the simulation count; the focus seat ignores --puct-simulations in that mode".into(),
        );
    }
    // A budget of 0 means "no deadline", which is meaningless for a mode whose
    // whole contract is the deadline.
    if search_mode == SearchMode::TimeBudgeted && decision_budget_ms == 0 {
        return Err("search-mode time-budgeted requires --decision-budget-ms > 0".into());
    }
    // The detail reserve only exists inside a decision budget: it is the part of
    // the budget held back from the search, so it cannot exceed half of it.
    if let Some(reserve) = detail_reserve_ms {
        if decision_budget_ms == 0 {
            return Err("--detail-reserve-ms requires --decision-budget-ms > 0".into());
        }
        if reserve > decision_budget_ms / 2 {
            return Err("--detail-reserve-ms must be at most half of --decision-budget-ms".into());
        }
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
        detail_reserve_ms,
        search_mode,
        mcts_simulations,
        time_budget_max_simulations,
        mcts_tree_depth,
        puct_simulations,
        puct_tree_depth,
        puct_c,
        puct_prior_temperature,
        puct_value_temperature,
        puct_leaf,
        puct_root_limit,
        puct_final_prior,
        baseline_puct_simulations,
        baseline_puct_tree_depth,
        baseline_puct_c,
        baseline_puct_prior_temperature,
        baseline_puct_value_temperature,
        baseline_puct_leaf,
        baseline_puct_root_limit,
        baseline_puct_final_prior,
        output,
    })
}

/// A stable identity for one PUCT configuration (#219). Historical runs all
/// logged the policy name `puct`, so two very different configurations are
/// indistinguishable in the archives; every new record carries these IDs so
/// an ablation never compares two unnamed variants.
fn puct_config_id(config: &PuctConfig) -> String {
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
struct SearchSettings {
    mode: SearchMode,
    mcts_simulations: usize,
    puct_simulations: usize,
    budget: Duration,
    time_budget_max_simulations: usize,
}

impl SearchSettings {
    fn rule_for(self, ai: Ai) -> Option<SearchRule> {
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
    fn limit_for(self, ai: Ai, search_deadline: Option<Instant>) -> Option<SearchLimit> {
        Some(self.rule_for(ai)?.resolve(search_deadline))
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
    eval_veg: &'a VariantEvaluationPolicy,
    eval_food: &'a VariantEvaluationPolicy,
    eval_veg_cond: &'a VariantEvaluationPolicy,
    eval_room: &'a VariantEvaluationPolicy,
    mcts: &'a MctsPolicy,
    /// PUCT configuration of the seat in this slot. The focus seat receives
    /// the candidate configuration in the candidate arm and the frozen
    /// baseline configuration in the baseline arm (#219); every opponent PUCT
    /// seat always receives the frozen baseline configuration.
    puct: &'a PuctPolicy,
    /// Frozen baseline PUCT configuration used by opponent PUCT seats.
    puct_baseline: &'a PuctPolicy,
    /// Stopping rule for the focus seat. `None` for heuristic policies and for
    /// the opponent seats, which always run the policy's own deterministic
    /// default.
    search: Option<SearchSettings>,
    decision_ns: RefCell<Vec<u64>>,
    focus_outcomes: RefCell<FocusOutcomes>,
}

/// Per-decision stopping rule template (#164). The absolute deadline is
/// resolved at the moment the decision starts.
#[derive(Clone, Copy, Debug)]
enum SearchRule {
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
    fn resolve(self, search_deadline: Option<Instant>) -> SearchLimit {
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
fn select_ai(
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
    let eval_veg = VariantEvaluationPolicy {
        variant: EvaluationVariant::VegetableFirst,
    };
    let eval_food = VariantEvaluationPolicy {
        variant: EvaluationVariant::FoodBuffer,
    };
    let eval_veg_cond = VariantEvaluationPolicy {
        variant: EvaluationVariant::VegetableConditional,
    };
    let eval_room = VariantEvaluationPolicy {
        variant: EvaluationVariant::GrowthRoomPriority,
    };
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
    // #219: the frozen baseline configuration. The baseline arm's focus seat
    // and every opponent PUCT seat play this policy, so a candidate variant is
    // always compared against a separately pinned PUCT, never against itself.
    let baseline_puct = PuctPolicy {
        config: PuctConfig {
            simulations: options.baseline_puct_simulations,
            tree_depth: options.baseline_puct_tree_depth,
            c_puct: options.baseline_puct_c,
            prior_temperature: options.baseline_puct_prior_temperature,
            value_temperature: options.baseline_puct_value_temperature,
            leaf_mode: options.baseline_puct_leaf,
            root_candidate_limit: options.baseline_puct_root_limit,
            final_prior_weight: options.baseline_puct_final_prior,
        },
    };
    let puct_leaf_name = match options.puct_leaf {
        LeafMode::Heuristic => "heuristic",
        LeafMode::Terminal => "terminal",
    };
    // A wall-clock deadline is only enforced in time-budgeted mode. In
    // simulation-capped mode the search length, not the clock, decides, so the
    // run stays reproducible on any machine.
    let deadline_enforced = options.search_mode == SearchMode::TimeBudgeted;
    let search_settings = SearchSettings {
        mode: options.search_mode,
        mcts_simulations: options.mcts_simulations,
        puct_simulations: options.puct_simulations,
        budget: Duration::from_millis(options.decision_budget_ms),
        time_budget_max_simulations: options.time_budget_max_simulations,
    };
    // #167 A: the whole-decision budget. `simulation-capped` is reproducible, so
    // it must not see a clock at all: it gets `None` and its decisions are
    // identical to the pre-#167 path. `time-budgeted` bounds selection **and**
    // details with one window, holding back the detail reserve from the search.
    // The budget is applied to the **focus seat only** (review P1): the opponent
    // seats must not become wall-clock dependent just because a budget exists.
    let budget_spec = if deadline_enforced {
        let limit = Duration::from_millis(options.decision_budget_ms);
        Some(match options.detail_reserve_ms {
            Some(reserve) => {
                DecisionBudgetSpec::with_reserve(limit, Duration::from_millis(reserve))
            }
            None => DecisionBudgetSpec::new(limit),
        })
    } else {
        None
    };
    let detail_reserve_ms = budget_spec
        .map(|spec| spec.reserve().as_millis().min(u128::from(u64::MAX)) as u64)
        .unwrap_or(0);
    // Derived, not caller-supplied: the reserve is whatever the budget template
    // actually resolved, so a record cannot claim a reserve that did not run.
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
                // The focus seat plays the arm's own PUCT configuration; the
                // fixed simulation count must therefore follow the same policy
                // (otherwise the recorded cap would disagree with what ran).
                let arm_is_candidate = arm_id == "candidate";
                let focus_puct = if arm_is_candidate {
                    &puct
                } else {
                    &baseline_puct
                };
                let focus_settings = SearchSettings {
                    puct_simulations: if arm_is_candidate {
                        options.puct_simulations
                    } else {
                        options.baseline_puct_simulations
                    },
                    ..search_settings
                };
                let selector = SeatSelector {
                    focus: seat,
                    focus_ai,
                    opponent_ai: options.opponent,
                    random: &random,
                    greedy: &greedy,
                    eval: &eval,
                    eval_veg: &eval_veg,
                    eval_food: &eval_food,
                    eval_veg_cond: &eval_veg_cond,
                    eval_room: &eval_room,
                    mcts: &mcts,
                    puct: focus_puct,
                    puct_baseline: &baseline_puct,
                    search: Some(focus_settings),
                    decision_ns: RefCell::new(Vec::new()),
                    focus_outcomes: RefCell::new(FocusOutcomes::default()),
                };
                let game_start = Instant::now();
                let timings = RefCell::new(DecisionTimings::new(options.decision_budget_ms));
                let seat_budget = budget_spec.map(|spec| BudgetedDecisions::focus_only(spec, seat));
                play_to_end_with_budget(
                    &mut game,
                    &selector,
                    details,
                    &SimpleHarvestDetails,
                    seat_budget,
                    |outcome| {
                        if outcome.player == seat {
                            timings.borrow_mut().record(outcome);
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
                let timings = timings.borrow();
                let whole_ns = timings.elapsed_ns();
                let budget_ns = options.decision_budget_ms * 1_000_000;
                let focus_timeout_count = if deadline_enforced {
                    whole_ns.iter().filter(|&&ns| ns > budget_ns).count()
                } else {
                    0
                };
                let focus_max_decision_ms = timings.max_whole_ms();
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
                    "focus_decisions": timings.count(),
                    "decision_budget_ms": options.decision_budget_ms,
                    "whole_decision_budget_enforced": deadline_enforced,
                    "detail_reserve_ms": detail_reserve_ms,
                    "mcts_deadline_active": focus_ai.is_search() && deadline_enforced,
                    "search_deadline_active": focus_ai.is_search() && deadline_enforced,
                    "search_mode": options.search_mode.as_str(),
                    "mcts_simulations_cap": options.mcts_simulations,
                    "time_budget_max_simulations": options.time_budget_max_simulations,
                    "search_budget_enforced": deadline_enforced,
                    "deterministic_policy_selection": !(focus_ai.is_search() && deadline_enforced),
                    "max_policy_select_ms": decision_ns.iter().copied().max().unwrap_or(0) as f64 / 1_000_000.0,
                    "p50_policy_select_ms": percentile_ms(&decision_ns, 0.50),
                    "p95_policy_select_ms": percentile_ms(&decision_ns, 0.95),
                    "p99_policy_select_ms": percentile_ms(&decision_ns, 0.99),
                    "focus_max_decision_ms": focus_max_decision_ms,
                    "focus_p95_decision_ms": timings.p95_whole_ms(),
                    "focus_p99_decision_ms": timings.p99_whole_ms(),
                    "focus_max_detail_ms": timings.max_detail_ms(),
                    "focus_p95_detail_ms": timings.p95_detail_ms(),
                    "focus_timeout_count": focus_timeout_count,
                    "focus_late_decision_count": timings.late_count(),
                    "focus_late_beyond_tolerance_count": timings.late_beyond_tolerance_count(),
                    "focus_budget_tolerance_ms": timings.budget_tolerance_ns() as f64 / 1_000_000.0,
                    "focus_max_decision_overshoot_ms": timings.max_overshoot_ms(),
                    "focus_decision_fallback_count": timings.fallback_count(),
                    "focus_selection_fallback_count": timings.selection_fallbacks(),
                    "focus_detail_fallback_count": timings.detail_fallbacks(),
                    "focus_detail_declined_count": timings.detail_declines(),
                    "focus_budget_exhausted_before_details_count": timings
                        .budget_exhausted_before_details(),
                    "focus_observed_all_decisions_within_budget": timings
                        .observed_all_decisions_within_budget(),
                    "focus_observed_all_decisions_within_budget_tolerance": timings
                        .observed_all_decisions_within_budget_tolerance(),
                    "focus_observed_all_decisions_within_10s": timings
                        .observed_all_decisions_within_10s(),
                    "timeout_count": outcomes.timeouts,
                    "fallback_count": outcomes.fallbacks,
                    "mcts_simulations_completed": outcomes.simulations,
                    "search_simulations_completed": outcomes.simulations,
                    "mcts_simulations_per_decision": if timings.count() == 0 {
                        0.0
                    } else {
                        outcomes.simulations as f64 / timings.count() as f64
                    },
                    "max_deadline_overshoot_ms": outcomes.overshoot_ms,
                    "max_search_elapsed_ms": outcomes.elapsed_ms,
                    "mcts_tree_depth": options.mcts_tree_depth,
                    "puct_leaf": puct_leaf_name,
                    // Candidate PUCT configuration identifiers (#219) are
                    // attached to the record below: the json! literal is at
                    // the macro recursion limit, so these three keys are
                    // inserted separately. The scalar puct_* fields above
                    // always describe the candidate configuration, even in
                    // baseline-arm rows; the baseline arm's focus
                    // configuration is identified by `focus_puct_config_id`.
                    "baseline_puct_simulations": options.baseline_puct_simulations,
                    "baseline_puct_tree_depth": options.baseline_puct_tree_depth,
                    "baseline_puct_c": options.baseline_puct_c,
                    "baseline_puct_prior_temperature": options
                        .baseline_puct_prior_temperature,
                    "baseline_puct_value_temperature": options
                        .baseline_puct_value_temperature,
                    "baseline_puct_leaf": match options.baseline_puct_leaf {
                        LeafMode::Heuristic => "heuristic",
                        LeafMode::Terminal => "terminal",
                    },
                    "baseline_puct_root_limit": options.baseline_puct_root_limit,
                    "baseline_puct_final_prior": options.baseline_puct_final_prior,
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
                let mut record = record;
                record["puct_config_id"] = serde_json::json!(puct_config_id(&puct.config));
                record["candidate_puct_config_id"] =
                    serde_json::json!(puct_config_id(&puct.config));
                record["baseline_puct_config_id"] =
                    serde_json::json!(puct_config_id(&baseline_puct.config));
                record["focus_puct_config_id"] =
                    serde_json::json!(puct_config_id(&focus_puct.config));
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
        assert_eq!(Ai::parse("eval_veg").unwrap(), Ai::EvalVeg);
        assert_eq!(Ai::parse("eval_food").unwrap(), Ai::EvalFood);
        assert_eq!(Ai::parse("eval_veg_cond").unwrap(), Ai::EvalVegConditional);
        assert_eq!(Ai::parse("eval_room").unwrap(), Ai::EvalRoom);
        assert!(Ai::Mcts.is_search() && Ai::Puct.is_search());
        assert!(!Ai::Eval.is_search() && !Ai::Random.is_search());
        assert_eq!(PuctConfig::default().leaf_mode, LeafMode::Terminal);
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

    fn settings(mode: SearchMode, mcts: usize, puct: usize) -> SearchSettings {
        SearchSettings {
            mode,
            mcts_simulations: mcts,
            puct_simulations: puct,
            budget: Duration::from_millis(250),
            time_budget_max_simulations: 100_000,
        }
    }

    fn fixed_count(rule: SearchRule) -> usize {
        match rule {
            SearchRule::Simulations(n) => n,
            SearchRule::TimeBudget { .. } => panic!("expected a fixed simulation rule"),
        }
    }

    /// Regression for the review finding on PR #166: a PUCT focus must use
    /// `--puct-simulations`, never MCTS's count, in simulation-capped mode.
    #[test]
    fn simulation_capped_uses_each_search_policys_own_count() {
        let shared = settings(SearchMode::SimulationCapped, 3, 7);
        assert_eq!(fixed_count(shared.rule_for(Ai::Mcts).unwrap()), 3);
        assert_eq!(fixed_count(shared.rule_for(Ai::Puct).unwrap()), 7);
        for heuristic in [
            Ai::Random,
            Ai::Greedy,
            Ai::Eval,
            Ai::EvalVeg,
            Ai::EvalFood,
            Ai::EvalVegConditional,
            Ai::EvalRoom,
        ] {
            assert!(
                shared.rule_for(heuristic).is_none(),
                "heuristics must not receive a search limit"
            );
        }
        // Unequal counts must not collapse to one value, whichever is larger.
        let reversed = settings(SearchMode::SimulationCapped, 9, 2);
        assert_eq!(fixed_count(reversed.rule_for(Ai::Mcts).unwrap()), 9);
        assert_eq!(fixed_count(reversed.rule_for(Ai::Puct).unwrap()), 2);
    }

    /// #219: two distinct PUCT configurations must never share a name.
    #[test]
    fn puct_config_id_encodes_every_frozen_parameter() {
        // #263: the shipped default is now the adopted pt6 configuration, so
        // the baseline ID in this test pins the explicit historical pt3 values
        // rather than reading them from PuctConfig::default().
        let mut config = PuctConfig {
            prior_temperature: 3.0,
            ..PuctConfig::default()
        };
        assert_eq!(
            puct_config_id(&config),
            "puct-s32-d2-c1.5-pt3-vt8-terminal-rl0-fp0",
            "the baseline ID freezes the historical configuration"
        );
        for (name, changed) in [
            ("simulations", {
                config.simulations = 17;
                puct_config_id(&config)
            }),
            ("tree_depth", {
                config.simulations = PuctConfig::default().simulations;
                config.tree_depth = 5;
                puct_config_id(&config)
            }),
            ("c_puct", {
                config.tree_depth = PuctConfig::default().tree_depth;
                config.c_puct = 0.75;
                puct_config_id(&config)
            }),
            ("prior_temperature", {
                config.c_puct = PuctConfig::default().c_puct;
                config.prior_temperature = 1.5;
                puct_config_id(&config)
            }),
            ("value_temperature", {
                config.prior_temperature = PuctConfig::default().prior_temperature;
                config.value_temperature = 4.0;
                puct_config_id(&config)
            }),
            ("root_candidate_limit", {
                config.value_temperature = PuctConfig::default().value_temperature;
                config.root_candidate_limit = 12;
                puct_config_id(&config)
            }),
            ("final_prior_weight", {
                config.root_candidate_limit = PuctConfig::default().root_candidate_limit;
                config.final_prior_weight = 0.5;
                puct_config_id(&config)
            }),
            ("leaf_mode", {
                config.final_prior_weight = PuctConfig::default().final_prior_weight;
                config.leaf_mode = LeafMode::Heuristic;
                puct_config_id(&config)
            }),
        ] {
            assert_ne!(
                changed, "puct-s32-d2-c1.5-pt3-vt8-terminal-rl0-fp0",
                "changing {name} must rename the config"
            );
        }
        assert!(puct_config_id(&config).contains("-heuristic-rl"));
    }

    /// #219: unset baseline flags fall back to the candidate configuration,
    /// so a pre-#219 command still describes the same policy it always did;
    /// set flags override only the baseline side.
    #[test]
    fn baseline_puct_defaults_track_the_candidate_configuration() {
        let base = [
            "--players",
            "2",
            "--seed-start",
            "1",
            "--block-start",
            "0",
            "--blocks",
            "1",
            "--candidate",
            "puct",
            "--baseline",
            "eval",
            "--opponent",
            "greedy",
            "--additional-action-space",
            "false",
            "--output",
            "out.jsonl",
        ]
        .map(String::from);
        let unset = parse_options_from(base.iter().cloned()).unwrap();
        assert_eq!(unset.baseline_puct_simulations, unset.puct_simulations);
        assert_eq!(unset.baseline_puct_tree_depth, unset.puct_tree_depth);
        assert_eq!(unset.baseline_puct_c, unset.puct_c);
        assert_eq!(
            unset.baseline_puct_prior_temperature,
            unset.puct_prior_temperature
        );
        assert_eq!(
            unset.baseline_puct_value_temperature,
            unset.puct_value_temperature
        );
        assert_eq!(unset.baseline_puct_leaf, unset.puct_leaf);
        assert_eq!(unset.baseline_puct_root_limit, unset.puct_root_limit);
        assert_eq!(unset.baseline_puct_final_prior, unset.puct_final_prior);

        let with_overrides = base
            .iter()
            .cloned()
            .chain([
                "--puct-c".to_string(),
                "2.5".to_string(),
                "--baseline-puct-simulations".to_string(),
                "32".to_string(),
                "--baseline-puct-tree-depth".to_string(),
                "2".to_string(),
                "--baseline-puct-c".to_string(),
                "1.5".to_string(),
                "--baseline-puct-prior-temp".to_string(),
                "3".to_string(),
                "--baseline-puct-value-temp".to_string(),
                "8".to_string(),
                "--baseline-puct-leaf".to_string(),
                "terminal".to_string(),
                "--baseline-puct-root-limit".to_string(),
                "0".to_string(),
                "--baseline-puct-final-prior".to_string(),
                "0".to_string(),
            ])
            .collect::<Vec<_>>();
        let overridden = parse_options_from(with_overrides.into_iter()).unwrap();
        assert_eq!(overridden.puct_c, 2.5);
        assert_eq!(overridden.baseline_puct_c, 1.5);
        assert_eq!(overridden.baseline_puct_simulations, 32);
        assert_eq!(overridden.baseline_puct_prior_temperature, 3.0);
    }

    #[test]
    fn partially_frozen_baseline_puct_is_rejected() {
        let base = [
            "--players",
            "2",
            "--seed-start",
            "1",
            "--block-start",
            "0",
            "--blocks",
            "1",
            "--candidate",
            "puct",
            "--baseline",
            "eval",
            "--opponent",
            "greedy",
            "--additional-action-space",
            "false",
            "--output",
            "out.jsonl",
        ]
        .map(String::from);
        // One flag alone cannot define the frozen configuration: refusing is
        // cheaper than a baseline that silently inherits a candidate variant.
        let args = base
            .iter()
            .cloned()
            .chain(["--baseline-puct-c".to_string(), "1.5".to_string()])
            .collect::<Vec<_>>();
        assert!(parse_options_from(args.into_iter()).is_err());
    }

    #[test]
    fn puct_vs_puct_requires_a_distinct_baseline_configuration() {
        let base = [
            "--players",
            "2",
            "--seed-start",
            "1",
            "--block-start",
            "0",
            "--blocks",
            "1",
            "--candidate",
            "puct",
            "--baseline",
            "puct",
            "--opponent",
            "puct",
            "--additional-action-space",
            "false",
            "--output",
            "out.jsonl",
        ]
        .map(String::from);
        // Identical configurations would make the baseline arm an exact copy
        // of the candidate arm: the run must fail closed.
        assert!(parse_options_from(base.iter().cloned()).is_err());
        // A frozen baseline equal to the candidate default is still "the same
        // configuration": rejected as a meaningless mirror.
        // Since #263 the candidate default is pt6, so the frozen baseline that
        // equals the candidate default is pt6 as well.
        let frozen_defaults = [
            "--baseline-puct-simulations",
            "32",
            "--baseline-puct-tree-depth",
            "2",
            "--baseline-puct-c",
            "1.5",
            "--baseline-puct-prior-temp",
            "6",
            "--baseline-puct-value-temp",
            "8",
            "--baseline-puct-leaf",
            "terminal",
            "--baseline-puct-root-limit",
            "0",
            "--baseline-puct-final-prior",
            "0",
        ]
        .map(String::from);
        let args = base
            .iter()
            .cloned()
            .chain(frozen_defaults.clone())
            .collect::<Vec<_>>();
        assert!(parse_options_from(args.into_iter()).is_err());
        // A candidate variant against the frozen defaults is accepted.
        let args = base
            .iter()
            .cloned()
            .chain(frozen_defaults)
            .chain(["--puct-c".to_string(), "2.5".to_string()])
            .collect::<Vec<_>>();
        assert!(parse_options_from(args.into_iter()).is_ok());
    }

    /// #219 review P2: in time-budgeted mode a simulations-only configuration
    /// difference is invisible to the focus seat and would be a null run.
    #[test]
    fn time_budgeted_puct_vs_puct_with_simulations_only_difference_is_rejected() {
        let mut base = vec![
            "--players",
            "2",
            "--seed-start",
            "1",
            "--block-start",
            "0",
            "--blocks",
            "1",
            "--candidate",
            "puct",
            "--baseline",
            "puct",
            "--opponent",
            "puct",
            "--additional-action-space",
            "false",
            "--search-mode",
            "time-budgeted",
            "--decision-budget-ms",
            "100",
            // Pin the candidate to the same prior temperature as the baseline
            // so the only configuration difference is the simulation count
            // (#263 changed the candidate default prior temperature to 6).
            "--puct-prior-temp",
            "3",
            "--output",
            "out.jsonl",
        ]
        .into_iter()
        .map(String::from)
        .collect::<Vec<_>>();
        base.extend([
            "--baseline-puct-simulations".to_string(),
            "64".to_string(),
            "--baseline-puct-tree-depth".to_string(),
            "2".to_string(),
            "--baseline-puct-c".to_string(),
            "1.5".to_string(),
            "--baseline-puct-prior-temp".to_string(),
            "3".to_string(),
            "--baseline-puct-value-temp".to_string(),
            "8".to_string(),
            "--baseline-puct-leaf".to_string(),
            "terminal".to_string(),
            "--baseline-puct-root-limit".to_string(),
            "0".to_string(),
            "--baseline-puct-final-prior".to_string(),
            "0".to_string(),
        ]);
        assert!(parse_options_from(base.iter().cloned()).is_err());
        // The same difference is meaningful in simulation-capped mode.
        let capped = base
            .iter()
            .enumerate()
            .map(|(i, arg)| {
                if arg == "time-budgeted" && i > 0 && base[i - 1] == "--search-mode" {
                    "simulation-capped".to_string()
                } else {
                    arg.clone()
                }
            })
            .collect::<Vec<_>>();
        assert!(parse_options_from(capped.into_iter()).is_ok());
    }

    #[test]
    fn baseline_puct_overrides_fail_closed_on_invalid_values() {
        let base = [
            "--players",
            "2",
            "--seed-start",
            "1",
            "--block-start",
            "0",
            "--blocks",
            "1",
            "--candidate",
            "puct",
            "--baseline",
            "eval",
            "--opponent",
            "greedy",
            "--additional-action-space",
            "false",
            "--output",
            "out.jsonl",
        ]
        .map(String::from);
        for (flag, value) in [
            ("--baseline-puct-simulations", "0"),
            ("--baseline-puct-tree-depth", "7"),
            ("--baseline-puct-c", "-1"),
            ("--baseline-puct-prior-temp", "0"),
            ("--baseline-puct-value-temp", "abc"),
            ("--baseline-puct-root-limit", "101"),
            ("--baseline-puct-final-prior", "11"),
            ("--baseline-puct-leaf", "rollout"),
        ] {
            let args = base
                .iter()
                .cloned()
                .chain([flag.to_string(), value.to_string()])
                .collect::<Vec<_>>();
            assert!(
                parse_options_from(args.into_iter()).is_err(),
                "{flag} {value} must be rejected"
            );
        }
    }

    /// Regression for the self-review finding in #167 A: the focus seat used
    /// the template clock (the whole decision budget), so the search consumed the
    /// detail reserve and every decision ended after its own budget with the
    /// detail policy skipped.
    #[test]
    fn time_budgeted_honours_the_search_window_reserve() {
        let shared = settings(SearchMode::TimeBudgeted, 3, 7);
        let window = Instant::now() + Duration::from_millis(30);
        for ai in [Ai::Mcts, Ai::Puct] {
            match shared.limit_for(ai, Some(window)).unwrap() {
                SearchLimit::TimeBudget {
                    deadline,
                    max_simulations,
                } => {
                    assert_eq!(deadline, window, "the reserve window must win");
                    assert_eq!(max_simulations, 100_000);
                }
                SearchLimit::Simulations(_) => {
                    panic!("time-budgeted must stay wall-clock bounded")
                }
            }
        }
        // The template clock is only a fallback when no whole-decision budget was
        // resolved (for example a direct policy call in a test).
        assert!(matches!(
            shared.limit_for(Ai::Mcts, None).unwrap(),
            SearchLimit::TimeBudget { .. }
        ));
        // A fixed count is never converted into a deadline.
        let capped = settings(SearchMode::SimulationCapped, 3, 7);
        assert!(matches!(
            capped.limit_for(Ai::Mcts, Some(window)).unwrap(),
            SearchLimit::Simulations(3)
        ));
    }

    #[test]
    fn time_budgeted_uses_the_shared_budget_for_both_search_policies() {
        let shared = settings(SearchMode::TimeBudgeted, 3, 7);
        for ai in [Ai::Mcts, Ai::Puct] {
            match shared.rule_for(ai).unwrap() {
                SearchRule::TimeBudget {
                    budget,
                    max_simulations,
                } => {
                    assert_eq!(budget, Duration::from_millis(250));
                    assert_eq!(max_simulations, 100_000);
                }
                SearchRule::Simulations(_) => {
                    panic!("time-budgeted must not fall back to a fixed count")
                }
            }
        }
    }
}
