//! Export reproducible self-play episodes as machine-readable JSONL.
//!
//! This is the small local driver for the M12 data layer. Large batches belong
//! on the dedicated GitHub Actions simulation workflow, not in the normal PR
//! CI (see `docs/github-actions-mass-simulation-m8-3.md`).
//!
//! Usage:
//!   episode_export --episodes 20 --seed 1000 --players 2 --out episodes.jsonl
//!   episode_export --cards --code-sha <git-sha>
use agricola_re_ai::cards::{verified_basic_card_set, CardSet};
use agricola_re_ai::episode_data::{
    inspect_dataset, record_self_play, to_jsonl, EpisodeMeta, EpisodeRecord,
};
use agricola_re_ai::evaluation::EvaluationPolicy;
use agricola_re_ai::model::Config;
use agricola_re_ai::policy::{CardChoiceDetails, SimpleDetails};

struct Options {
    episodes: usize,
    seed: u64,
    players: usize,
    additional_action_space: bool,
    cards: bool,
    out: String,
    code_sha: String,
}

fn parse() -> Result<Options, String> {
    let mut options = Options {
        episodes: 1,
        seed: 1,
        players: 2,
        additional_action_space: false,
        cards: false,
        out: "episodes.jsonl".into(),
        code_sha: "unknown".into(),
    };
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    let mut index = 0;
    while index < args.len() {
        let value = |index: usize| -> Result<String, String> {
            args.get(index + 1)
                .cloned()
                .ok_or_else(|| format!("{} needs a value", args[index]))
        };
        match args[index].as_str() {
            "--episodes" => {
                options.episodes = value(index)?
                    .parse()
                    .map_err(|e: std::num::ParseIntError| e.to_string())?
            }
            "--seed" => {
                options.seed = value(index)?
                    .parse()
                    .map_err(|e: std::num::ParseIntError| e.to_string())?
            }
            "--players" => {
                options.players = value(index)?
                    .parse()
                    .map_err(|e: std::num::ParseIntError| e.to_string())?
            }
            "--out" => options.out = value(index)?,
            "--code-sha" => options.code_sha = value(index)?,
            "--cards" => {
                options.cards = true;
                index += 1;
                continue;
            }
            "--additional-action-space" => {
                options.additional_action_space = true;
                index += 1;
                continue;
            }
            other => return Err(format!("unknown argument {other}")),
        }
        index += 2;
    }
    if options.episodes == 0 {
        return Err("--episodes must be at least 1".into());
    }
    if !(2..=4).contains(&options.players) {
        return Err("--players must be 2, 3 or 4".into());
    }
    Ok(options)
}

fn main() {
    let options = match parse() {
        Ok(options) => options,
        Err(error) => {
            eprintln!("episode_export: {error}");
            std::process::exit(2);
        }
    };
    let card_set = if options.cards {
        match verified_basic_card_set(options.players) {
            Ok(set) => set,
            Err(error) => {
                eprintln!("episode_export: {error}");
                std::process::exit(2);
            }
        }
    } else {
        CardSet::default()
    };
    let detail_policy: &dyn agricola_re_ai::policy::DetailPolicy = if card_set.enabled() {
        &CardChoiceDetails
    } else {
        &SimpleDetails
    };
    let card_mode = if options.cards {
        "verified_basic"
    } else {
        "none"
    };
    let meta = EpisodeMeta::new(
        options.code_sha.clone(),
        card_mode,
        vec!["evaluation".to_string(); options.players],
    );
    let mut records: Vec<EpisodeRecord> = Vec::new();
    for offset in 0..options.episodes {
        let config = Config {
            seed: options.seed.wrapping_add(offset as u64),
            players: options.players,
            additional_action_space_enabled: options.additional_action_space,
        };
        match record_self_play(
            config,
            card_set.clone(),
            meta.clone(),
            &EvaluationPolicy,
            detail_policy,
        ) {
            Ok(record) => records.push(record),
            Err(error) => {
                eprintln!(
                    "episode_export: seed {} failed: {error}",
                    options.seed + offset as u64
                );
                std::process::exit(1);
            }
        }
    }
    let jsonl = match to_jsonl(&records) {
        Ok(jsonl) => jsonl,
        Err(error) => {
            eprintln!("episode_export: {error}");
            std::process::exit(1);
        }
    };
    if let Err(error) = std::fs::write(&options.out, jsonl) {
        eprintln!("episode_export: cannot write {}: {error}", options.out);
        std::process::exit(1);
    }
    let report = inspect_dataset(&records);
    println!(
        "wrote {} episodes to {} (finished {}, players {:?}, duplicate seeds {}, corrupt {})",
        report.total,
        options.out,
        report.finished,
        report.player_counts,
        report.duplicate_seeds.len(),
        report.corrupt.len()
    );
    if !report.is_clean() {
        std::process::exit(1);
    }
}
