//! Reproducible seat-rotated, cardless AI comparisons for M8-3.
//! A JSONL row is one completed game. Each seed block contains every
//! candidate seat for both comparison arms. This is NOT M7 MCTS evaluation.
use agricola_re_ai::evaluation::EvaluationPolicy;
use agricola_re_ai::policy::{play_to_end, GreedyPolicy, RandomPolicy, SimpleDetails, SpacePolicy};
use agricola_re_ai::score::standings;
use agricola_re_ai::{Config, Game, Observation};
use rand_chacha::ChaCha8Rng;
use serde_json::json;
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::fs::File;
use std::io::{BufWriter, Write};
use std::time::Instant;

struct Options {
    players: usize,
    seed_start: u64,
    block_start: usize,
    blocks: usize,
    candidate: String,
    baseline: String,
    opponent: String,
    additional_action_space: bool,
    output: String,
}

fn option<'a>(values: &'a BTreeMap<String, String>, key: &str) -> Result<&'a str, String> {
    values
        .get(key)
        .map(String::as_str)
        .ok_or_else(|| format!("missing --{key}"))
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
    let candidate = option(&values, "candidate")?.to_string();
    let baseline = option(&values, "baseline")?.to_string();
    let opponent = option(&values, "opponent")?.to_string();
    let additional_action_space = match option(&values, "additional-action-space")? {
        "true" => true,
        "false" => false,
        _ => return Err("additional-action-space must be true or false".into()),
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
    for name in [&candidate, &baseline, &opponent] {
        if !["random", "greedy", "eval"].contains(&name.as_str()) {
            return Err(format!("unsupported AI: {name}"));
        }
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
        output,
    })
}

struct SeatSelector<'a> {
    focus: usize,
    focus_policy: &'a dyn SpacePolicy,
    opponent_policy: &'a dyn SpacePolicy,
    decision_ns: RefCell<Vec<u64>>,
}

impl SpacePolicy for SeatSelector<'_> {
    fn select(
        &self,
        obs: &Observation,
        legal: &[agricola_re_ai::model::Space],
        rng: &mut ChaCha8Rng,
    ) -> Option<agricola_re_ai::model::Space> {
        let before = Instant::now();
        let selected = if obs.player_id == self.focus {
            self.focus_policy.select(obs, legal, rng)
        } else {
            self.opponent_policy.select(obs, legal, rng)
        };
        let ns = before.elapsed().as_nanos().min(u128::from(u64::MAX)) as u64;
        self.decision_ns.borrow_mut().push(ns);
        selected
    }
}

fn policy<'a>(
    name: &str,
    random: &'a RandomPolicy,
    greedy: &'a GreedyPolicy,
    eval: &'a EvaluationPolicy,
) -> &'a dyn SpacePolicy {
    match name {
        "random" => random,
        "greedy" => greedy,
        "eval" => eval,
        _ => unreachable!("validated by parse_options"),
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
    let opponent = policy(&options.opponent, &random, &greedy, &eval);
    let start = Instant::now();
    for relative_block in 0..options.blocks {
        let block_index = options.block_start + relative_block;
        let seed = options.seed_start + block_index as u64;
        for (arm_id, focus_name) in [
            ("candidate", options.candidate.as_str()),
            ("baseline", options.baseline.as_str()),
        ] {
            let chosen = policy(focus_name, &random, &greedy, &eval);
            for seat in 0..options.players {
                let config = Config {
                    players: options.players,
                    seed,
                    additional_action_space_enabled: options.additional_action_space,
                };
                let mut game = Game::new(config.clone());
                let selector = SeatSelector {
                    focus: seat,
                    focus_policy: chosen,
                    opponent_policy: opponent,
                    decision_ns: RefCell::new(Vec::new()),
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
                let max_decision_ms =
                    decision_ns.iter().copied().max().unwrap_or(0) as f64 / 1_000_000.0;
                let record = json!({
                    "schema_version": 1,
                    "block_index": block_index,
                    "seed": seed,
                    "arm_id": arm_id,
                    "focus_policy": focus_name,
                    "opponent_policy": options.opponent,
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
                    "max_policy_select_ms": max_decision_ms,
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
}
