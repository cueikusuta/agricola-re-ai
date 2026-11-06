//! Reproducible seat-rotated, cardless AI comparisons for M8-3.
//! A JSONL row is one completed game. Each seed block contains every
//! candidate seat for both comparison arms. The optional M7 MCTS arm runs
//! under a per-decision wall-clock budget and records whether it timed out or
//! fell back to the guaranteed-legal evaluation policy.
use agricola_re_ai::evaluation::EvaluationPolicy;
use agricola_re_ai::mcts::{MctsOutcome, MctsPolicy};
use agricola_re_ai::model::Space;
use agricola_re_ai::policy::{play_to_end, GreedyPolicy, RandomPolicy, SimpleDetails, SpacePolicy};
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
    decision_budget_ms: u64,
    mcts_simulations: usize,
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
    if !(1..=60_000).contains(&decision_budget_ms) {
        return Err("decision-budget-ms must be 1..=60000".into());
    }
    if mcts_simulations == 0 || mcts_simulations > 100_000 {
        return Err("mcts-simulations must be 1..=100000".into());
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
        tree_depth: 2,
    };
    let budget = Duration::from_millis(options.decision_budget_ms);
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
                    budget: focus_ai.is_mcts().then_some(budget),
                    decision_ns: RefCell::new(Vec::new()),
                    focus_outcomes: RefCell::new(FocusOutcomes::default()),
                };
                let game_start = Instant::now();
                play_to_end(&mut game, &selector, &SimpleDetails)
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
                    "decision_budget_ms": options.decision_budget_ms,
                    "max_policy_select_ms": decision_ns.iter().copied().max().unwrap_or(0) as f64 / 1_000_000.0,
                    "p50_policy_select_ms": percentile_ms(&decision_ns, 0.50),
                    "p95_policy_select_ms": percentile_ms(&decision_ns, 0.95),
                    "p99_policy_select_ms": percentile_ms(&decision_ns, 0.99),
                    "timeout_count": outcomes.timeouts,
                    "fallback_count": outcomes.fallbacks,
                    "mcts_simulations_completed": outcomes.simulations,
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
}
