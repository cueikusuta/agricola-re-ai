//! Design-aligned evaluation harness (PR #400 `docs/ai-evaluation-design.md` §2, §3, §5).
//!
//! One seed block is `players` seat rotations of the **candidate** arm plus
//! `players` seat rotations of the **baseline** arm. The candidate arm puts the
//! candidate game policy in the focus seat and the opponent (normally the
//! frozen baseline) in the other seats; the baseline arm is the mirror control.
//! This is the seat-rotated, paired design the legacy `paired_eval` binary uses,
//! extended with:
//!
//! * the 10-7 simultaneous draft before the game (test-fixture pool only until
//!   the official 4-player gate is met),
//! * a `run-manifest.json` with the pre-registration fields and condition id,
//! * per-pick timing / fallback and coordinator-only final hands,
//! * an explicit smoke mode so a short check cannot be mistaken for a
//!   measurement.
//!
//! Rows are written as `schema_version: 2` JSONL under `shard-0/`. The legacy
//! `paired_eval` remains the schema-1 path for its existing workflows.
#![recursion_limit = "512"]
use agricola_re_ai::cards::{verified_basic_card_set, CardSet};
use agricola_re_ai::decision_budget::{percentile_ms, DecisionBudgetSpec, DecisionTimings};
use agricola_re_ai::draft_ai::DraftAiKind;
use agricola_re_ai::draft_game::start_game_from_draft;
use agricola_re_ai::eval_harness::{
    condition_id, draft_policy_id, first_credit, fixture_4p_card_set, fixture_draft_setup,
    game_policy_id, play_draft, pool_gate_snapshot, puct_config_id, Ai, FocusOutcomes,
    SearchSettings, SeatSelector,
};
use agricola_re_ai::evaluation::{EvaluationPolicy, EvaluationVariant, VariantEvaluationPolicy};
use agricola_re_ai::harvest::SimpleHarvestDetails;
use agricola_re_ai::mcts::MctsPolicy;
use agricola_re_ai::mcts_puct::{LeafMode, PuctConfig, PuctPolicy};
use agricola_re_ai::policy::{
    play_to_end_with_budget, BudgetedDecisions, CardChoiceDetails, DetailPolicy, GreedyPolicy,
    RandomPolicy, SimpleDetails,
};
use agricola_re_ai::score::{standings, standings_with_cards};
use agricola_re_ai::{Config, Game};
use serde_json::{json, Value};
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::fs::{self, File};
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const DEFAULT_DECISION_BUDGET_MS: u64 = 10_000;
const DEFAULT_SIMULATIONS: usize = 64;
const DEFAULT_TIME_BUDGET_MAX_SIMULATIONS: usize = 100_000;
/// A smoke run may not grow into a measurement by accident.
const MAX_SMOKE_BLOCKS: usize = 4;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Cards {
    None,
    VerifiedBasic2p,
    Fixture4p,
}

impl Cards {
    fn parse(name: &str) -> Result<Self, String> {
        match name {
            "none" => Ok(Cards::None),
            "verified_basic_2p" => Ok(Cards::VerifiedBasic2p),
            "fixture_4p" => Ok(Cards::Fixture4p),
            _ => Err(format!("unsupported card condition: {name}")),
        }
    }
    fn name(self) -> &'static str {
        match self {
            Cards::None => "none",
            Cards::VerifiedBasic2p => "verified_basic_2p",
            Cards::Fixture4p => "fixture_4p",
        }
    }
    fn is_fixture(self) -> bool {
        self == Cards::Fixture4p
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DraftPlan {
    None,
    Draft107,
}

impl DraftPlan {
    fn parse(name: &str) -> Result<Self, String> {
        match name {
            "none" => Ok(DraftPlan::None),
            "draft_10_7_simultaneous" => Ok(DraftPlan::Draft107),
            _ => Err(format!("unsupported draft mode: {name}")),
        }
    }
    fn id(self) -> &'static str {
        match self {
            DraftPlan::None => "none",
            DraftPlan::Draft107 => "draft_10_7_simultaneous",
        }
    }
    fn enabled(self) -> bool {
        self != DraftPlan::None
    }
}

struct Options {
    output: PathBuf,
    players: usize,
    seed_start: u64,
    draft_seed_start: u64,
    block_start: usize,
    blocks: usize,
    candidate: Ai,
    baseline: Ai,
    opponent: Ai,
    candidate_draft_ai: DraftAiKind,
    baseline_draft_ai: DraftAiKind,
    draft: DraftPlan,
    cards: Cards,
    additional_action_space: bool,
    decision_budget_ms: u64,
    detail_reserve_ms: Option<u64>,
    search_mode: String,
    mcts_simulations: usize,
    mcts_tree_depth: usize,
    puct: PuctConfig,
    smoke: bool,
    code_sha: Option<String>,
}

fn parse_draft_ai(name: &str) -> Result<DraftAiKind, String> {
    match name {
        "random" => Ok(DraftAiKind::Random),
        "card_value" => Ok(DraftAiKind::CardValue),
        "synergy" => Ok(DraftAiKind::Synergy),
        _ => Err(format!("unsupported draft AI: {name}")),
    }
}

fn required<'a>(values: &'a BTreeMap<String, String>, key: &str) -> Result<&'a str, String> {
    values
        .get(key)
        .map(String::as_str)
        .ok_or_else(|| format!("missing --{key}"))
}

fn parse_number<T: std::str::FromStr>(
    values: &BTreeMap<String, String>,
    key: &str,
    default: T,
    error: &str,
) -> Result<T, String> {
    match values.get(key) {
        Some(value) => value.parse::<T>().map_err(|_| error.to_string()),
        None => Ok(default),
    }
}

fn parse_options_from(args: impl Iterator<Item = String>) -> Result<Options, String> {
    const KNOWN: &[&str] = &[
        "output",
        "players",
        "seed-start",
        "draft-seed-start",
        "block-start",
        "blocks",
        "candidate",
        "baseline",
        "opponent",
        "candidate-draft-ai",
        "baseline-draft-ai",
        "draft-mode",
        "cards",
        "additional-action-space",
        "decision-budget-ms",
        "detail-reserve-ms",
        "search-mode",
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
        "smoke",
        "code-sha",
    ];
    let mut values = BTreeMap::new();
    let mut args = args;
    while let Some(arg) = args.next() {
        let key = arg
            .strip_prefix("--")
            .ok_or_else(|| format!("invalid argument: {arg}"))?;
        if !KNOWN.contains(&key) {
            return Err(format!("unsupported option: --{key}"));
        }
        // `--smoke` is a flag; every other option takes a value.
        if key == "smoke" {
            if values.insert(key.to_string(), "true".to_string()).is_some() {
                return Err("duplicate option: --smoke".into());
            }
            continue;
        }
        let value = args
            .next()
            .ok_or_else(|| format!("missing value for --{key}"))?;
        if values.insert(key.to_string(), value).is_some() {
            return Err(format!("duplicate option: --{key}"));
        }
    }
    let output = PathBuf::from(required(&values, "output")?);
    let players = required(&values, "players")?
        .parse::<usize>()
        .map_err(|_| "invalid players".to_string())?;
    let seed_start = required(&values, "seed-start")?
        .parse::<u64>()
        .map_err(|_| "invalid seed-start".to_string())?;
    let draft_seed_start = parse_number(
        &values,
        "draft-seed-start",
        seed_start,
        "invalid draft-seed-start",
    )?;
    let block_start = parse_number(&values, "block-start", 0usize, "invalid block-start")?;
    let blocks = required(&values, "blocks")?
        .parse::<usize>()
        .map_err(|_| "invalid blocks".to_string())?;
    let candidate = Ai::parse(required(&values, "candidate")?)?;
    let baseline = Ai::parse(required(&values, "baseline")?)?;
    let opponent = match values.get("opponent") {
        Some(name) => Ai::parse(name)?,
        None => baseline,
    };
    let candidate_draft_ai = parse_draft_ai(
        values
            .get("candidate-draft-ai")
            .map(String::as_str)
            .unwrap_or("card_value"),
    )?;
    let baseline_draft_ai = parse_draft_ai(
        values
            .get("baseline-draft-ai")
            .map(String::as_str)
            .unwrap_or("card_value"),
    )?;
    let draft = DraftPlan::parse(
        values
            .get("draft-mode")
            .map(String::as_str)
            .unwrap_or("none"),
    )?;
    let cards = Cards::parse(values.get("cards").map(String::as_str).unwrap_or("none"))?;
    let additional_action_space = match values
        .get("additional-action-space")
        .map(String::as_str)
        .unwrap_or("false")
    {
        "true" => true,
        "false" => false,
        _ => return Err("additional-action-space must be true or false".into()),
    };
    let decision_budget_ms = parse_number(
        &values,
        "decision-budget-ms",
        DEFAULT_DECISION_BUDGET_MS,
        "invalid decision-budget-ms",
    )?;
    let detail_reserve_ms = match values.get("detail-reserve-ms") {
        Some(value) => Some(
            value
                .parse::<u64>()
                .map_err(|_| "invalid detail-reserve-ms".to_string())?,
        ),
        None => None,
    };
    let search_mode = values
        .get("search-mode")
        .map(String::as_str)
        .unwrap_or("simulation-capped")
        .to_string();
    let mcts_simulations = parse_number(
        &values,
        "mcts-simulations",
        DEFAULT_SIMULATIONS,
        "invalid mcts-simulations",
    )?;
    let mcts_tree_depth = parse_number(
        &values,
        "mcts-tree-depth",
        2usize,
        "invalid mcts-tree-depth",
    )?;
    let default_puct = PuctConfig::default();
    let puct = PuctConfig {
        simulations: parse_number(
            &values,
            "puct-simulations",
            default_puct.simulations,
            "invalid puct-simulations",
        )?,
        tree_depth: parse_number(
            &values,
            "puct-tree-depth",
            default_puct.tree_depth,
            "invalid puct-tree-depth",
        )?,
        c_puct: parse_number(&values, "puct-c", default_puct.c_puct, "invalid puct-c")?,
        prior_temperature: parse_number(
            &values,
            "puct-prior-temp",
            default_puct.prior_temperature,
            "invalid puct-prior-temp",
        )?,
        value_temperature: parse_number(
            &values,
            "puct-value-temp",
            default_puct.value_temperature,
            "invalid puct-value-temp",
        )?,
        leaf_mode: match values.get("puct-leaf").map(String::as_str).unwrap_or(
            match default_puct.leaf_mode {
                LeafMode::Heuristic => "heuristic",
                LeafMode::Terminal => "terminal",
            },
        ) {
            "heuristic" => LeafMode::Heuristic,
            "terminal" => LeafMode::Terminal,
            _ => return Err("puct-leaf must be heuristic or terminal".into()),
        },
        root_candidate_limit: parse_number(
            &values,
            "puct-root-limit",
            default_puct.root_candidate_limit,
            "invalid puct-root-limit",
        )?,
        final_prior_weight: parse_number(
            &values,
            "puct-final-prior",
            default_puct.final_prior_weight,
            "invalid puct-final-prior",
        )?,
    };
    let smoke = values.contains_key("smoke");
    let code_sha = values.get("code-sha").cloned();
    Ok(Options {
        output,
        players,
        seed_start,
        draft_seed_start,
        block_start,
        blocks,
        candidate,
        baseline,
        opponent,
        candidate_draft_ai,
        baseline_draft_ai,
        draft,
        cards,
        additional_action_space,
        decision_budget_ms,
        detail_reserve_ms,
        search_mode,
        mcts_simulations,
        mcts_tree_depth,
        puct,
        smoke,
        code_sha,
    })
}

/// Fail closed on every combination the design says is a different experiment
/// or simply impossible today.
fn validate(options: &Options) -> Result<(), String> {
    if !(2..=4).contains(&options.players) {
        return Err("players must be 2, 3 or 4".into());
    }
    if options.blocks < 1 {
        return Err("blocks must be positive".into());
    }
    let last_block = (options.blocks - 1) as u64;
    if options
        .seed_start
        .checked_add(options.block_start as u64)
        .and_then(|start| start.checked_add(last_block))
        .is_none()
    {
        return Err("seed-start + block-start + blocks - 1 overflows u64".into());
    }
    if options
        .draft_seed_start
        .checked_add(options.block_start as u64)
        .and_then(|start| start.checked_add(last_block))
        .is_none()
    {
        return Err("draft-seed-start + block-start + blocks - 1 overflows u64".into());
    }
    if options.puct.simulations == 0 || options.mcts_simulations == 0 {
        return Err("simulation counts must be positive".into());
    }
    if !(1..=60_000).contains(&options.decision_budget_ms) {
        return Err("decision-budget-ms must be in 1..=60000".into());
    }
    if options.search_mode != "simulation-capped" && options.search_mode != "time-budgeted" {
        return Err(format!("unsupported search mode: {}", options.search_mode));
    }
    if options.smoke && options.blocks > MAX_SMOKE_BLOCKS {
        return Err(format!(
            "a smoke run is capped at {MAX_SMOKE_BLOCKS} blocks; use a full run for more"
        ));
    }
    let baseline_puct = PuctConfig::default();
    let same_game_policy = options.candidate == options.baseline;
    let same_draft_policy = options.candidate_draft_ai == options.baseline_draft_ai;
    let distinct_puct = options.candidate == Ai::Puct
        && options.baseline == Ai::Puct
        && puct_config_id(&options.puct) != puct_config_id(&baseline_puct);
    // A draft-contribution arm (design §4.4) keeps the game policy fixed and
    // changes only the draft policy, so "same game policy" alone is not an
    // identical-condition error.
    if same_game_policy && (!options.draft.enabled() || same_draft_policy) && !distinct_puct {
        return Err(
            "candidate and baseline must differ: change the game policy, a candidate --puct-* override, or the draft policy"
                .into(),
        );
    }
    if options
        .detail_reserve_ms
        .map(|reserve| reserve >= options.decision_budget_ms)
        == Some(true)
    {
        return Err("detail-reserve-ms must be smaller than decision-budget-ms".into());
    }
    if options.draft.enabled() {
        if options.cards != Cards::Fixture4p {
            return Err(
                "draft_10_7_simultaneous requires --cards fixture_4p until the official 4-player pool meets G1"
                    .into(),
            );
        }
        if options.players != 4 {
            return Err("draft_10_7_simultaneous is evaluated at four players".into());
        }
    } else if options.cards == Cards::Fixture4p {
        return Err(
            "--cards fixture_4p is only meaningful with --draft-mode draft_10_7_simultaneous"
                .into(),
        );
    }
    if options.cards == Cards::VerifiedBasic2p && options.players != 2 {
        return Err("the verified basic pool supports two players only".into());
    }
    if options.cards == Cards::Fixture4p && options.players != 4 {
        return Err("fixture_4p is the four-player draft fixture".into());
    }
    Ok(())
}

fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}

fn policy_id(ai: Ai, config: &PuctConfig) -> String {
    game_policy_id(ai, config)
}

fn write_error(dir: &Path, block: usize, arm: &str, seat: usize, message: &str) {
    let path = dir.join("errors.jsonl");
    let Ok(mut file) = fs::OpenOptions::new().create(true).append(true).open(path) else {
        return;
    };
    let record = json!({
        "block_index": block,
        "arm_id": arm,
        "candidate_seat": seat,
        "error": message,
    });
    let _ = writeln!(file, "{record}");
}

fn run(options: &Options) -> Result<(), String> {
    validate(options)?;
    fs::create_dir_all(options.output.join("shard-0")).map_err(|error| error.to_string())?;
    let matches_path = options.output.join("shard-0").join("matches.jsonl");
    let manifest_path = options.output.join("run-manifest.json");
    if matches_path.exists() || manifest_path.exists() {
        return Err(format!(
            "output directory already contains a run: {}",
            options.output.display()
        ));
    }

    // Card condition. The fixture pool is the only 4-player pool the code can
    // offer before G1 is met; it is labelled as such in the manifest.
    let card_set = match options.cards {
        Cards::None => CardSet::default(),
        Cards::VerifiedBasic2p => verified_basic_card_set(2)?,
        Cards::Fixture4p => fixture_4p_card_set()?,
    };
    let card_set_hash = card_set.content_hash();
    let card_set_count = card_set.definitions.len();
    let details: &dyn DetailPolicy = if card_set.enabled() {
        &CardChoiceDetails
    } else {
        &SimpleDetails
    };

    let baseline_puct = baseline_puct();
    let candidate_puct = options.puct;
    let candidate_id = policy_id(options.candidate, &candidate_puct);
    let baseline_id = policy_id(options.baseline, &baseline_puct);
    let opponent_id = policy_id(options.opponent, &baseline_puct);
    let opponent_is_baseline = options.opponent == options.baseline;
    let head_to_head = opponent_is_baseline;

    // Environment and gate snapshots (design §3.5, §5.2, U12). The code SHA is
    // resolved before the condition hash so the identity can include it.
    let code_sha = options
        .code_sha
        .clone()
        .or_else(|| std::env::var("EVAL_CODE_SHA").ok())
        .or_else(|| std::env::var("GITHUB_SHA").ok());
    let provenance_incomplete = code_sha.is_none();
    if !options.smoke && provenance_incomplete {
        return Err(
            "a non-smoke run needs an immutable code identity: pass --code-sha or set EVAL_CODE_SHA / GITHUB_SHA"
                .into(),
        );
    }
    let code_sha_value = code_sha.clone().unwrap_or_else(|| "unknown".into());
    let budget_description = format!(
        "{}:{}ms:reserve={}:mcts={}x{}:puct={}:max_sims={}:draft_pick={}ms",
        options.search_mode,
        options.decision_budget_ms,
        options
            .detail_reserve_ms
            .map(|value| value.to_string())
            .unwrap_or_else(|| "derived".into()),
        options.mcts_simulations,
        options.mcts_tree_depth,
        puct_config_id(&candidate_puct),
        DEFAULT_TIME_BUDGET_MAX_SIMULATIONS,
        options.decision_budget_ms
    );
    let condition_parts = BTreeMap::from([
        ("players".to_string(), options.players.to_string()),
        (
            "ruleset".to_string(),
            "agricola-revised-2016-draft-right-v1".to_string(),
        ),
        ("deck_content_hash".to_string(), card_set_hash.clone()),
        (
            "card_condition".to_string(),
            options.cards.name().to_string(),
        ),
        ("draft_mode".to_string(), options.draft.id().to_string()),
        (
            "additional_action_space".to_string(),
            options.additional_action_space.to_string(),
        ),
        ("opponent_profile".to_string(), opponent_id.clone()),
        ("baseline_policy".to_string(), baseline_id.clone()),
        ("compute_budget".to_string(), budget_description.clone()),
        ("engine_version".to_string(), code_sha_value.clone()),
        ("code_sha".to_string(), code_sha_value.clone()),
    ]);
    let condition_id = condition_id(&condition_parts);

    let pool_gate = pool_gate_snapshot(options.players, code_sha.as_deref());
    let g1_met = pool_gate
        .get("g1_met")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    // The final mode is a measurement, not a constant: it becomes available
    // only when the official 4-player pool exists and the fixture is not in use.
    // U1 still blocks the official protocol, so the status is reported as
    // unavailable rather than silently claimed.
    let official_pool_ready =
        options.players == 4 && options.draft.enabled() && !options.cards.is_fixture() && g1_met;
    // U1 (#155) is an open protocol decision, not a threshold: even with the
    // official pool the final mode stays unavailable until a human decides it.
    let draft_protocol_decided = false;
    let official_final_mode = official_pool_ready && draft_protocol_decided;
    let official_blockers = {
        let mut blockers = Vec::new();
        if options.players != 4 || !options.draft.enabled() {
            blockers.push("not the final 4-player 10-7 mode");
        }
        if options.cards.is_fixture() {
            blockers.push("test-fixture pool: functional verification only");
        }
        if !g1_met {
            blockers.push("G1 40+40 official verified pool is not met");
        }
        if !draft_protocol_decided {
            blockers.push("U1 (#155) 10-7 draft conventions remain undecided");
        }
        blockers
    };
    let this_run = format!(
        "{}-s{}-b{}-{}-{}",
        &condition_id[..12],
        options.seed_start,
        options.block_start,
        options.draft.id(),
        options.cards.name()
    );

    // PUCT and search settings.
    let puct = PuctPolicy {
        config: candidate_puct,
    };
    let baseline_policy = PuctPolicy {
        config: baseline_puct,
    };
    let mcts = MctsPolicy {
        simulations: options.mcts_simulations,
        tree_depth: options.mcts_tree_depth,
    };
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
    let deadline_enforced = options.search_mode == "time-budgeted";
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
    let search_settings = SearchSettings {
        mode: if deadline_enforced {
            agricola_re_ai::mcts::SearchMode::TimeBudgeted
        } else {
            agricola_re_ai::mcts::SearchMode::SimulationCapped
        },
        mcts_simulations: options.mcts_simulations,
        puct_simulations: options.puct.simulations,
        budget: Duration::from_millis(options.decision_budget_ms),
        time_budget_max_simulations: DEFAULT_TIME_BUDGET_MAX_SIMULATIONS,
    };

    let started_at = unix_now();
    let experiment_start = Instant::now();

    // Pre-registration is written **before** any game. The final manifest
    // carries its SHA-256, so a later edit to the plan cannot pass unnoticed.
    let details_policy_id = if card_set.enabled() {
        "card_choice_details"
    } else {
        "simple_details"
    };
    let experiment_id = format!(
        "eval-{}-{}-{}-{}",
        options.players,
        options.cards.name(),
        options.draft.id(),
        &condition_id[..12]
    );
    let arms_json = json!({
        "candidate": {
            "game_policy_id": candidate_id,
            "policy": options.candidate.name(),
            "puct_config_id": if options.candidate == Ai::Puct {
                Some(puct_config_id(&candidate_puct))
            } else {
                None
            },
            "draft_policy_id": if options.draft.enabled() {
                Some(draft_policy_id(options.candidate_draft_ai))
            } else {
                None
            },
            "details_policy_id": details_policy_id,
        },
        "baseline": {
            "game_policy_id": baseline_id,
            "policy": options.baseline.name(),
            "puct_config_id": if options.baseline == Ai::Puct {
                Some(puct_config_id(&baseline_puct))
            } else {
                None
            },
            "draft_policy_id": if options.draft.enabled() {
                Some(draft_policy_id(options.baseline_draft_ai))
            } else {
                None
            },
            "details_policy_id": details_policy_id,
        },
        "opponent": {
            "game_policy_id": opponent_id,
            "policy": options.opponent.name(),
            "stopping_rule": "policy default (fixed simulations); not time-budgeted",
            "note": if head_to_head {
                "opponent seats use the frozen baseline configuration"
            } else {
                "common-opponent benchmark: this run is NOT head-to-head"
            },
        },
    });
    let compute_budget_json = json!({
        "search_mode": options.search_mode,
        "decision_budget_ms": options.decision_budget_ms,
        "detail_reserve_ms": detail_reserve_ms,
        "mcts_simulations": options.mcts_simulations,
        "mcts_tree_depth": options.mcts_tree_depth,
        "puct_simulations": options.puct.simulations,
        "puct_config_id": puct_config_id(&candidate_puct),
        "time_budget_max_simulations": DEFAULT_TIME_BUDGET_MAX_SIMULATIONS,
        "draft_pick_budget_ms": options.decision_budget_ms,
        "draft_pick_budget_enforced": false,
        "opponent_seats_time_budgeted": false,
        "opponent_stopping_rule": "policy default (fixed simulations)",
        "budget_description": budget_description,
    });
    let seeds_json = json!({
        "seed_start": options.seed_start,
        "draft_seed_start": options.draft_seed_start,
        "first_block": options.block_start,
        "blocks": options.blocks,
        "shards": 1,
        "split_version": null,
        "test_range_locked": null,
        "block_seed": "seed_start + block_index",
        "draft_block_seed": "draft_seed_start + block_index",
        "bootstrap": {"method": "percentile-block", "iterations": 10000, "seed": 20261106},
    });
    let draft_json = json!({
        "mode": options.draft.id(),
        "ruleset_id": if options.draft.enabled() {
            Some("eval-4p-10-7-right-v1")
        } else {
            None
        },
        "deck_version": if options.draft.enabled() {
            Some("eval-fixture-4p-v1")
        } else {
            None
        },
        "draft_ruleset": if options.draft.enabled() {
            json!({
                "direction_occupation": "right",
                "direction_minor": "right",
                "start_player_stream": "independent (draft seed xor constant)",
                "pick_order": "not decided (U1 / #155)",
            })
        } else {
            Value::Null
        },
        "protocol_open_items": if options.draft.enabled() {
            vec!["U1: #155 simultaneous-pick order/timeout conventions are not decided"]
        } else {
            Vec::new()
        },
    });
    let environment_json = json!({
        "host_os": std::env::consts::OS,
        "cpu_model": std::env::var("EVAL_CPU_MODEL").ok(),
        "build_profile": if cfg!(debug_assertions) { "debug" } else { "release" },
        "rustc_version": option_env!("RUSTC_VERSION"),
        "rustc_version_source": "captured only when the build sets RUSTC_VERSION",
        "measurement_started_at_unix": started_at,
    });
    let preregistration = json!({
        "registered": false,
        "primary_comparison": if head_to_head {
            "candidate focus seat vs baseline opponents (head-to-head) + baseline mirror arm"
        } else {
            "common-opponent benchmark (NOT head-to-head; no mirror control)"
        },
        "decision_rule_version": "lexicographic-first-v1 (design §3.3)",
        "interim_plan": "not registered: smoke / exploratory run only",
        "max_blocks": null,
        "note": "This harness records the plan; a strength claim needs an experiment registered before the run."
    });
    let pre_manifest = json!({
        "schema_version": 2,
        "harness": "eval_harness",
        "experiment_id": experiment_id.clone(),
        "run_id": this_run.clone(),
        "status": "running",
        "condition_id": condition_id.clone(),
        "condition_parts": condition_parts.clone(),
        "code_sha": code_sha_value.clone(),
        "provenance_incomplete": provenance_incomplete,
        "players": options.players,
        "cards": options.cards.name(),
        "card_set_count": card_set_count,
        "card_set_hash": card_set_hash.clone(),
        "draft": draft_json.clone(),
        "arms": arms_json.clone(),
        "compute_budget": compute_budget_json.clone(),
        "seeds": seeds_json.clone(),
        "preregistration": preregistration.clone(),
        "pool_gate_snapshot": pool_gate.clone(),
        "official_final_mode": official_final_mode,
        "official_final_mode_blocked_by": official_blockers.clone(),
        "environment": environment_json.clone(),
        "comparison_design": {
            "head_to_head": head_to_head,
            "mirror_baseline": head_to_head,
            "opponent_is_baseline": opponent_is_baseline,
        },
        "started_at_unix": started_at,
    });
    let preregistration_bytes =
        serde_json::to_vec_pretty(&pre_manifest).map_err(|error| error.to_string())?;
    let preregistration_sha256 = agricola_re_ai::dataset_split::sha256_hex(&preregistration_bytes);
    fs::write(
        options.output.join("run-manifest.pre.json"),
        &preregistration_bytes,
    )
    .map_err(|error| error.to_string())?;

    let file = File::create(&matches_path).map_err(|error| error.to_string())?;
    let mut writer = BufWriter::new(file);

    for relative_block in 0..options.blocks {
        let block_index = options.block_start + relative_block;
        let seed = options
            .seed_start
            .checked_add(block_index as u64)
            .ok_or("block seed overflow")?;
        let draft_seed = options
            .draft_seed_start
            .checked_add(block_index as u64)
            .ok_or("draft seed overflow")?;
        for (arm_id, focus_ai) in [
            ("candidate", options.candidate),
            ("baseline", options.baseline),
        ] {
            let arm_is_candidate = arm_id == "candidate";
            let focus_puct = if arm_is_candidate {
                &puct
            } else {
                &baseline_policy
            };
            for seat in 0..options.players {
                let result = run_one_game(
                    options,
                    block_index,
                    seed,
                    draft_seed,
                    arm_id,
                    arm_is_candidate,
                    seat,
                    focus_ai,
                    focus_puct,
                    &baseline_policy,
                    &search_settings,
                    budget_spec,
                    deadline_enforced,
                    detail_reserve_ms,
                    &card_set,
                    details,
                    &random,
                    &greedy,
                    &eval,
                    &eval_veg,
                    &eval_food,
                    &eval_veg_cond,
                    &eval_room,
                    &mcts,
                    &candidate_id,
                    &baseline_id,
                    &opponent_id,
                    &condition_id,
                );
                match result {
                    Ok(record) => {
                        serde_json::to_writer(&mut writer, &record)
                            .map_err(|error| error.to_string())?;
                        writer.write_all(b"\n").map_err(|error| error.to_string())?;
                    }
                    Err(error) => {
                        write_error(&options.output, block_index, arm_id, seat, &error);
                        return Err(format!(
                            "block {block_index} arm {arm_id} seat {seat}: {error}"
                        ));
                    }
                }
            }
        }
    }
    writer.flush().map_err(|error| error.to_string())?;

    let manifest = json!({
        "schema_version": 2,
        "harness": "eval_harness",
        "experiment_id": experiment_id,
        "run_id": this_run,
        "status": "finished",
        "condition_id": condition_id,
        "condition_parts": condition_parts,
        "code_sha": code_sha_value,
        "provenance_incomplete": provenance_incomplete,
        "engine_version": code_sha_value,
        "package_version": env!("CARGO_PKG_VERSION"),
        "official_pool_ready": official_pool_ready,
        "players": options.players,
        "additional_action_space": options.additional_action_space,
        "run_mode": if options.smoke { "smoke" } else { "unregistered" },
        "smoke_blocks_cap": MAX_SMOKE_BLOCKS,
        "strength_claim_allowed": false,
        "cards": options.cards.name(),
        "card_set_count": card_set_count,
        "card_set_hash": card_set_hash,
        "draft": draft_json,
        "arms": arms_json,
        "compute_budget": compute_budget_json,
        "seeds": seeds_json,
        "preregistration": preregistration,
        "preregistration_file": "run-manifest.pre.json",
        "preregistration_sha256": preregistration_sha256,
        "pool_gate_snapshot": pool_gate,
        "official_final_mode": official_final_mode,
        "official_final_mode_blocked_by": official_blockers,
        "comparison_design": {
            "head_to_head": head_to_head,
            "mirror_baseline": head_to_head,
            "opponent_is_baseline": opponent_is_baseline,
        },
        "environment": environment_json,
        "started_at_unix": started_at,
        "finished_at_unix": unix_now(),
        "elapsed_seconds": experiment_start.elapsed().as_secs_f64(),
        "attempt": 1,
        "artifact_url": null,
        "artifact": {
            "matches": "shard-0/matches.jsonl",
            "row_schema_version": 2,
            "shards": 1,
        },
        "unimplemented": [
            "interference_proxy (design §1.4/U2): operational definition undecided",
            "discarded_private_ids_seat_attribution (design §5.3): flat list, engine change needed",
            "draft pick deadline enforcement (design §3.5): observed only, not enforced",
            "L1 draft-only proxy metrics (design §4.3/U10): out of scope for this chapter",
            "dataset_split / G0 binding (design §2.3): not wired into the harness yet",
            "seed registry collision check (design §2.3/U6): requires a registered range"
        ],
        "row_count": options.blocks * options.players * 2,
    });
    let mut manifest_file = File::create(&manifest_path).map_err(|error| error.to_string())?;
    serde_json::to_writer_pretty(&mut manifest_file, &manifest)
        .map_err(|error| error.to_string())?;
    manifest_file
        .write_all(b"\n")
        .map_err(|error| error.to_string())?;
    eprintln!(
        "completed {} games in {:.3}s; mode={}; condition={}; output={}",
        options.blocks * options.players * 2,
        experiment_start.elapsed().as_secs_f64(),
        if options.smoke { "smoke" } else { "full" },
        condition_id,
        options.output.display()
    );
    Ok(())
}

fn baseline_puct() -> PuctConfig {
    PuctConfig::default()
}

#[allow(clippy::too_many_arguments)]
fn run_one_game(
    options: &Options,
    block_index: usize,
    seed: u64,
    draft_seed: u64,
    arm_id: &str,
    arm_is_candidate: bool,
    seat: usize,
    focus_ai: Ai,
    focus_puct: &PuctPolicy,
    baseline_policy: &PuctPolicy,
    search_settings: &SearchSettings,
    budget_spec: Option<DecisionBudgetSpec>,
    deadline_enforced: bool,
    detail_reserve_ms: u64,
    card_set: &CardSet,
    details: &dyn DetailPolicy,
    random: &RandomPolicy,
    greedy: &GreedyPolicy,
    eval: &EvaluationPolicy,
    eval_veg: &VariantEvaluationPolicy,
    eval_food: &VariantEvaluationPolicy,
    eval_veg_cond: &VariantEvaluationPolicy,
    eval_room: &VariantEvaluationPolicy,
    mcts: &MctsPolicy,
    candidate_id: &str,
    baseline_id: &str,
    opponent_id: &str,
    condition_id: &str,
) -> Result<Value, String> {
    let config = Config {
        players: options.players,
        seed,
        additional_action_space_enabled: options.additional_action_space,
    };
    let mut draft_wall_ms = 0.0;
    let (mut game, draft_record) = if options.draft.enabled() {
        let setup = fixture_draft_setup(draft_seed)?;
        let seat_ais = (0..options.players)
            .map(|other| {
                if other == seat {
                    if arm_is_candidate {
                        options.candidate_draft_ai
                    } else {
                        options.baseline_draft_ai
                    }
                } else {
                    options.baseline_draft_ai
                }
            })
            .collect::<Vec<_>>();
        let draft_started = Instant::now();
        let draft_run = play_draft(&setup, &seat_ais)?;
        draft_wall_ms = draft_started.elapsed().as_secs_f64() * 1000.0;
        let state = draft_run.state.clone();
        let game = start_game_from_draft(&state, card_set.clone(), config.clone())?;
        let hands = state.final_hands();
        let draft_record = Some((draft_run, state, hands));
        (game, draft_record)
    } else {
        (
            Game::with_cards(config.clone(), card_set.clone())
                .map_err(|error| error.to_string())?,
            None,
        )
    };

    let focus_puct_config = focus_puct.config;
    let focus_settings = SearchSettings {
        puct_simulations: focus_puct_config.simulations,
        ..*search_settings
    };
    let selector = SeatSelector {
        focus: seat,
        focus_ai,
        opponent_ai: options.opponent,
        random,
        greedy,
        eval,
        eval_veg,
        eval_food,
        eval_veg_cond,
        eval_room,
        mcts,
        puct: focus_puct,
        puct_baseline: baseline_policy,
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
    .map_err(|error| error.to_string())?;
    if !game.public.finished {
        return Err("game did not finish".into());
    }
    let summary = if card_set.enabled() {
        standings_with_cards(&game.public.players, &game.card_set)
    } else {
        standings(&game.public.players)
    };
    let hash = game.state_hash();
    // A draft seeds explicit hands and a publicly drawn start player, so the
    // replay must rebuild from that setup rather than re-dealing from the seed.
    let (replay_hands, replay_start) = match &draft_record {
        Some((_, state, hands)) => (
            Some(
                hands
                    .iter()
                    .map(|(occupations, minors)| {
                        occupations.iter().chain(minors.iter()).cloned().collect()
                    })
                    .collect::<Vec<Vec<String>>>(),
            ),
            Some(state.start_player),
        ),
        None => (None, None),
    };
    let replay = Game::replay_with_setup_and_start_player(
        config,
        card_set.clone(),
        replay_hands,
        replay_start,
        &game.log.events,
    )
    .map_err(|error| format!("replay: {error}"))?;
    if replay.state_hash() != hash {
        return Err("replay mismatch".into());
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
    let budget_ns = options.decision_budget_ms * 1_000_000;
    let focus_timeout_count = if deadline_enforced {
        timings
            .elapsed_ns()
            .iter()
            .filter(|&&ns| ns > budget_ns)
            .count()
    } else {
        0
    };
    let game_policy_by_seat = (0..options.players)
        .map(|other| {
            if other == seat {
                if arm_is_candidate {
                    candidate_id.to_string()
                } else {
                    baseline_id.to_string()
                }
            } else {
                // Non-focus seats are the fixed opponent in both arms. When the
                // opponent is the baseline (the mirror design) this equals the
                // baseline id; a common-opponent run keeps its own profile.
                opponent_id.to_string()
            }
        })
        .collect::<Vec<_>>();
    let (draft_fields, pick_fields) = match &draft_record {
        Some((draft_run, state, hands)) => {
            let focus_ns = &draft_run.pick_ns_by_seat[seat];
            let focus_max_ms = focus_ns.iter().copied().max().unwrap_or(0) as f64 / 1_000_000.0;
            let focus_timeouts = focus_ns.iter().filter(|&&ns| ns > budget_ns).count();
            let draft_policy_by_seat = (0..options.players)
                .map(|other| {
                    if arm_is_candidate && other == seat {
                        draft_policy_id(options.candidate_draft_ai)
                    } else {
                        draft_policy_id(options.baseline_draft_ai)
                    }
                })
                .collect::<Vec<_>>();
            (
                json!({
                    "draft_mode": options.draft.id(),
                    "draft_ruleset_id": state.ruleset.id,
                    "draft_deck_version": state.deck_version,
                    "draft_policy_id_by_seat": draft_policy_by_seat,
                    "draft_replay_hash": draft_run.replay_hash.clone(),
                    "draft_state_hash": state.state_hash(),
                    "draft_actions": &state.actions,
                    "draft_event_count": state.actions.len() as u64,
                    "start_player": state.start_player,
                    "final_hands_by_seat": hands
                        .iter()
                        .map(|(occupations, minors)| json!({
                            "occupations": occupations,
                            "minor_improvements": minors,
                        }))
                        .collect::<Vec<_>>(),
                    "discarded_private_ids": &state.discarded,
                }),
                json!({
                    "focus_pick_max_ms": focus_max_ms,
                    "focus_pick_p95_ms": percentile_ms(focus_ns, 0.95),
                    "pick_timeout_count": focus_timeouts,
                    "pick_illegal_fallback_count": draft_run.fallbacks_by_seat[seat],
                    "draft_pick_budget_enforced": false,
                    "focus_observed_all_picks_within_budget": focus_timeouts == 0,
                }),
            )
        }
        None => (
            json!({
                "draft_mode": "none",
                "draft_ruleset_id": Value::Null,
                "draft_deck_version": Value::Null,
                "draft_policy_id_by_seat": Value::Null,
                "draft_replay_hash": Value::Null,
                "draft_state_hash": Value::Null,
                "draft_actions": Value::Null,
                "draft_event_count": Value::Null,
                "start_player": Value::Null,
                "final_hands_by_seat": Value::Null,
                "discarded_private_ids": Value::Null,
            }),
            json!({
                "focus_pick_max_ms": Value::Null,
                "focus_pick_p95_ms": Value::Null,
                "pick_timeout_count": Value::Null,
                "pick_illegal_fallback_count": Value::Null,
                "draft_pick_budget_enforced": Value::Null,
                "focus_observed_all_picks_within_budget": Value::Null,
            }),
        ),
    };
    let mut record = json!({
        "schema_version": 2,
        "condition_id": condition_id,
        "block_index": block_index,
        "seed": seed,
        "draft_seed": if options.draft.enabled() { json!(draft_seed) } else { Value::Null },
        "arm_id": arm_id,
        "focus_policy": focus_ai.name(),
        "opponent_policy": options.opponent.name(),
        "candidate_seat": seat,
        "players": options.players,
        "additional_action_space": options.additional_action_space,
        "cards": options.cards.name(),
        "card_set_count": card_set.definitions.len(),
        "card_set_hash": card_set.content_hash(),
        "candidate_puct_config_id": puct_config_id(&options.puct),
        "baseline_puct_config_id": puct_config_id(&PuctConfig::default()),
        "puct_config_id": puct_config_id(&options.puct),
        "focus_puct_config_id": if focus_ai == Ai::Puct {
            json!(puct_config_id(&focus_puct_config))
        } else {
            Value::Null
        },
        // The scalar puct_* fields always describe the **candidate** config,
        // matching schema v1. The focus seat's actual config is identified by
        // `focus_puct_config_id`, and the frozen baseline by `baseline_puct_*`.
        "puct_simulations_config": options.puct.simulations,
        "puct_tree_depth": options.puct.tree_depth,
        "puct_c": options.puct.c_puct,
        "puct_prior_temperature": options.puct.prior_temperature,
        "puct_value_temperature": options.puct.value_temperature,
        "puct_leaf": match options.puct.leaf_mode {
            LeafMode::Heuristic => "heuristic",
            LeafMode::Terminal => "terminal",
        },
        "puct_root_limit": options.puct.root_candidate_limit,
        "puct_final_prior_weight": options.puct.final_prior_weight,
        "baseline_puct_simulations": PuctConfig::default().simulations,
        "baseline_puct_tree_depth": PuctConfig::default().tree_depth,
        "baseline_puct_c": PuctConfig::default().c_puct,
        "baseline_puct_prior_temperature": PuctConfig::default().prior_temperature,
        "baseline_puct_value_temperature": PuctConfig::default().value_temperature,
        "baseline_puct_leaf": match PuctConfig::default().leaf_mode {
            LeafMode::Heuristic => "heuristic",
            LeafMode::Terminal => "terminal",
        },
        "baseline_puct_root_limit": PuctConfig::default().root_candidate_limit,
        "baseline_puct_final_prior": PuctConfig::default().final_prior_weight,
        "game_policy_id_by_seat": game_policy_by_seat,
        "finished": true,
        "rounds_completed": game.public.round as usize,
        "scores_by_seat": scores,
        "ranks_by_seat": summary.rank,
        "first_place_credit": first_credit(&summary.rank, seat)?,
        "expected_rank_observation": summary.rank[seat],
        "score_margin": own_score as f64 - others_mean,
        "interference_proxy": Value::Null,
        "decisions": decision_ns.len(),
        "focus_decisions": timings.count(),
        "decision_budget_ms": options.decision_budget_ms,
        "whole_decision_budget_enforced": deadline_enforced,
        "detail_reserve_ms": detail_reserve_ms,
        "mcts_deadline_active": focus_ai.is_search() && deadline_enforced,
        "search_deadline_active": focus_ai.is_search() && deadline_enforced,
        "search_mode": options.search_mode,
        "mcts_simulations_cap": options.mcts_simulations,
        "time_budget_max_simulations": DEFAULT_TIME_BUDGET_MAX_SIMULATIONS,
        "search_budget_enforced": deadline_enforced,
        "deterministic_policy_selection": !(focus_ai.is_search() && deadline_enforced),
        "max_policy_select_ms": decision_ns.iter().copied().max().unwrap_or(0) as f64 / 1_000_000.0,
        "p50_policy_select_ms": percentile_ms(&decision_ns, 0.50),
        "p95_policy_select_ms": percentile_ms(&decision_ns, 0.95),
        "p99_policy_select_ms": percentile_ms(&decision_ns, 0.99),
        "focus_max_decision_ms": timings.max_whole_ms(),
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
        "policy_select_only": true,
        "game_start_player": game.public.start_player,
        "initial_state_hash": game
            .log
            .events
            .first()
            .map(|event| event.before.clone())
            .unwrap_or_else(|| hash.clone()),
        "draft_wall_ms": draft_wall_ms,
        "total_wall_ms": game_start.elapsed().as_secs_f64() * 1000.0,
        "replay_hash": hash,
        "replay_verified": true,
    });
    for (key, value) in draft_fields.as_object().into_iter().flatten() {
        record[key.as_str()] = value.clone();
    }
    for (key, value) in pick_fields.as_object().into_iter().flatten() {
        record[key.as_str()] = value.clone();
    }
    Ok(record)
}

fn main() {
    let options = std::env::args()
        .skip(1)
        .collect::<Vec<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    if let Err(error) = parse_options_from(options.into_iter()).and_then(|options| run(&options)) {
        eprintln!("eval harness failed: {error}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use agricola_re_ai::eval_harness::SearchRule;

    fn base_args() -> Vec<String> {
        vec![
            "--output".into(),
            "unused".into(),
            "--players".into(),
            "4".into(),
            "--seed-start".into(),
            "700000".into(),
            "--blocks".into(),
            "1".into(),
            "--candidate".into(),
            "eval".into(),
            "--baseline".into(),
            "greedy".into(),
            "--search-mode".into(),
            "simulation-capped".into(),
        ]
    }

    #[test]
    fn parses_the_final_mode_shape() {
        let mut args = base_args();
        args.extend([
            "--draft-mode".into(),
            "draft_10_7_simultaneous".into(),
            "--cards".into(),
            "fixture_4p".into(),
            "--candidate-draft-ai".into(),
            "synergy".into(),
            "--baseline-draft-ai".into(),
            "random".into(),
            "--smoke".into(),
        ]);
        let options = parse_options_from(args.into_iter()).unwrap();
        assert_eq!(options.players, 4);
        assert_eq!(options.draft, DraftPlan::Draft107);
        assert_eq!(options.cards, Cards::Fixture4p);
        assert_eq!(options.candidate_draft_ai, DraftAiKind::Synergy);
        assert!(options.smoke);
        validate(&options).unwrap();
    }

    #[test]
    fn smoke_is_capped_and_draft_requires_the_fixture_pool() {
        let mut args = base_args();
        args.extend(["--smoke".into(), "--blocks".into(), "5".into()]);
        // The duplicate --blocks is rejected by the parser before the cap.
        assert!(parse_options_from(args.into_iter()).is_err());

        let mut args = base_args();
        for index in (0..args.len()).step_by(2) {
            if args[index] == "--blocks" {
                args[index + 1] = "5".into();
            }
        }
        args.push("--smoke".into());
        let options = parse_options_from(args.into_iter()).unwrap();
        assert!(validate(&options).is_err());

        let mut args = base_args();
        args.extend(["--draft-mode".into(), "draft_10_7_simultaneous".into()]);
        let options = parse_options_from(args.into_iter()).unwrap();
        assert!(
            validate(&options).is_err(),
            "draft without the fixture pool"
        );
    }

    #[test]
    fn identical_policies_are_rejected_unless_puct_configs_differ() {
        let mut args = base_args();
        for index in (0..args.len()).step_by(2) {
            if args[index] == "--baseline" {
                args[index + 1] = "eval".into();
            }
        }
        let options = parse_options_from(args.into_iter()).unwrap();
        assert!(validate(&options).is_err());

        let mut args = base_args();
        for index in (0..args.len()).step_by(2) {
            if args[index] == "--candidate" {
                args[index + 1] = "puct".into();
            }
            if args[index] == "--baseline" {
                args[index + 1] = "puct".into();
            }
        }
        let options = parse_options_from(args.into_iter()).unwrap();
        assert!(validate(&options).is_err(), "identical PUCT configs");
        let mut args = base_args();
        for index in (0..args.len()).step_by(2) {
            if args[index] == "--candidate" {
                args[index + 1] = "puct".into();
            }
            if args[index] == "--baseline" {
                args[index + 1] = "puct".into();
            }
        }
        args.extend(["--puct-prior-temp".into(), "3".into()]);
        let options = parse_options_from(args.into_iter()).unwrap();
        validate(&options).unwrap();
    }

    #[test]
    fn a_draft_only_candidate_is_a_valid_contribution_arm() {
        // Design §4.4: the game policy stays fixed and only the draft policy
        // changes, so the same-game-policy rejection must not fire.
        let mut args = base_args();
        args.extend([
            "--draft-mode".into(),
            "draft_10_7_simultaneous".into(),
            "--cards".into(),
            "fixture_4p".into(),
            "--candidate-draft-ai".into(),
            "synergy".into(),
            "--baseline-draft-ai".into(),
            "card_value".into(),
        ]);
        let options = parse_options_from(args.into_iter()).unwrap();
        validate(&options).unwrap();
    }

    #[test]
    fn identical_draft_and_game_policies_are_rejected() {
        let mut args = base_args();
        for index in (0..args.len()).step_by(2) {
            if args[index] == "--baseline" {
                args[index + 1] = "eval".into();
            }
        }
        args.extend([
            "--draft-mode".into(),
            "draft_10_7_simultaneous".into(),
            "--cards".into(),
            "fixture_4p".into(),
            "--candidate-draft-ai".into(),
            "synergy".into(),
            "--baseline-draft-ai".into(),
            "synergy".into(),
        ]);
        let options = parse_options_from(args.into_iter()).unwrap();
        assert!(validate(&options).is_err());
    }

    #[test]
    fn seed_overflow_is_rejected_before_any_game() {
        let mut args = base_args();
        for index in (0..args.len()).step_by(2) {
            if args[index] == "--seed-start" {
                args[index + 1] = u64::MAX.to_string();
            }
        }
        args.extend(["--block-start".into(), "1".into()]);
        let options = parse_options_from(args.into_iter()).unwrap();
        assert!(validate(&options).is_err());
    }

    #[test]
    fn a_zero_decision_budget_is_rejected() {
        let mut args = base_args();
        args.extend(["--decision-budget-ms".into(), "0".into()]);
        let options = parse_options_from(args.into_iter()).unwrap();
        assert!(validate(&options).is_err());
    }

    #[test]
    fn search_settings_keep_each_policys_own_simulation_count() {
        let options = parse_options_from(base_args().into_iter()).unwrap();
        let settings = SearchSettings {
            mode: agricola_re_ai::mcts::SearchMode::SimulationCapped,
            mcts_simulations: 3,
            puct_simulations: 7,
            budget: Duration::from_millis(10),
            time_budget_max_simulations: 100,
        };
        assert!(matches!(
            settings.rule_for(Ai::Mcts),
            Some(SearchRule::Simulations(3))
        ));
        assert!(matches!(
            settings.rule_for(Ai::Puct),
            Some(SearchRule::Simulations(7))
        ));
        assert!(settings.rule_for(Ai::Eval).is_none());
        assert_eq!(options.blocks, 1);
    }
}
