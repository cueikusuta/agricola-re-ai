//! Mode-wide cardless simulator verification CLI.
//!
//! Plays every requested seed in one mode (`--players` x `--additional-action-space`)
//! and writes per-game evidence plus a summary that the workflow gates on.
//! See `src/verification.rs` for what each check proves.
use agricola_re_ai::verification::{
    verify_game, ModeSpec, VerifiedPolicy, VERIFICATION_SCHEMA_VERSION,
};
use serde::Serialize;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Instant;

#[derive(Default, Serialize)]
struct CheckTotals {
    passed: usize,
    failed: usize,
}

#[derive(Serialize)]
struct Failure {
    seed: u64,
    checks: Vec<String>,
    details: Vec<String>,
}

#[derive(Serialize)]
struct Summary {
    schema_version: u32,
    players: usize,
    additional_action_space_enabled: bool,
    policy: &'static str,
    seeds_requested: usize,
    seeds_run: usize,
    seed_start: u64,
    shard: usize,
    shards: usize,
    passed_games: usize,
    failed_games: usize,
    elapsed_seconds: f64,
    mean_game_millis: f64,
    checks: BTreeMap<&'static str, CheckTotals>,
    failures: Vec<Failure>,
    commit_sha: String,
    binary_version: &'static str,
}

fn usage() -> &'static str {
    "Usage: mode_verify --players 2|3|4 --seeds N [--seed-start S] \
     [--policy random|greedy|eval] [--additional-action-space] \
     [--out DIR] [--shard I --shards K]"
}

fn fail_usage(message: &str) -> ! {
    eprintln!("{message}\n{}", usage());
    std::process::exit(2)
}

fn main() {
    let mut players: Option<usize> = None;
    let mut seeds: Option<usize> = None;
    let mut seed_start = 0u64;
    let mut policy = VerifiedPolicy::Eval;
    let mut extra = false;
    let mut out = PathBuf::from("results/mode-verify");
    let mut shard = 0usize;
    let mut shards = 1usize;
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--players" => {
                players = Some(
                    it.next()
                        .unwrap_or_else(|| fail_usage("--players requires 2, 3 or 4"))
                        .parse()
                        .unwrap_or_else(|_| fail_usage("--players requires 2, 3 or 4")),
                )
            }
            "--seeds" => {
                seeds = Some(
                    it.next()
                        .unwrap_or_else(|| fail_usage("--seeds requires N"))
                        .parse()
                        .unwrap_or_else(|_| fail_usage("--seeds requires a positive integer")),
                )
            }
            "--seed-start" => {
                seed_start = it
                    .next()
                    .unwrap_or_else(|| fail_usage("--seed-start requires S"))
                    .parse()
                    .unwrap_or_else(|_| fail_usage("--seed-start requires an unsigned integer"))
            }
            "--policy" => {
                let value = it
                    .next()
                    .unwrap_or_else(|| fail_usage("--policy requires a value"));
                policy = VerifiedPolicy::parse(value).unwrap_or_else(|e| fail_usage(&e))
            }
            "--additional-action-space" => extra = true,
            "--out" => {
                out = PathBuf::from(
                    it.next()
                        .unwrap_or_else(|| fail_usage("--out requires a directory")),
                )
            }
            "--shard" => {
                shard = it
                    .next()
                    .unwrap_or_else(|| fail_usage("--shard requires I"))
                    .parse()
                    .unwrap_or_else(|_| fail_usage("--shard requires an index"))
            }
            "--shards" => {
                shards = it
                    .next()
                    .unwrap_or_else(|| fail_usage("--shards requires K"))
                    .parse()
                    .unwrap_or_else(|_| fail_usage("--shards requires a count"))
            }
            "--help" | "-h" => {
                println!("{}", usage());
                return;
            }
            other => fail_usage(&format!("unknown argument {other}")),
        }
    }
    let players = players.unwrap_or_else(|| fail_usage("--players is required"));
    let seeds = seeds.unwrap_or_else(|| fail_usage("--seeds is required"));
    if seeds == 0 {
        fail_usage("--seeds must be greater than zero");
    }
    if shards == 0 || shard >= shards {
        fail_usage("--shard must be in 0..--shards and --shards must be positive");
    }
    let mode = ModeSpec::new(players, extra).unwrap_or_else(|error| fail_usage(&error));
    if let Err(error) = std::fs::create_dir_all(&out) {
        eprintln!("cannot create {}: {error}", out.display());
        std::process::exit(1);
    }

    let started = Instant::now();
    let mut checks: BTreeMap<&'static str, CheckTotals> = BTreeMap::new();
    let mut failures = Vec::new();
    let mut passed_games = 0usize;
    let mut seeds_run = 0usize;
    let mut journal = String::new();
    for index in 0..seeds {
        if index % shards != shard {
            continue;
        }
        let seed = seed_start + index as u64;
        let report = verify_game(mode, seed, policy);
        seeds_run += 1;
        if report.passed() {
            passed_games += 1;
        } else {
            failures.push(Failure {
                seed,
                checks: report
                    .failed_check_names()
                    .iter()
                    .map(|name| name.to_string())
                    .collect(),
                details: report
                    .checks
                    .iter()
                    .filter(|check| !check.passed)
                    .map(|check| format!("{}: {}", check.name, check.detail))
                    .collect(),
            });
        }
        for check in &report.checks {
            let totals = checks.entry(check.name).or_default();
            if check.passed {
                totals.passed += 1;
            } else {
                totals.failed += 1;
            }
        }
        journal.push_str(&serde_json::to_string(&report).expect("report JSON"));
        journal.push('\n');
    }
    let elapsed = started.elapsed().as_secs_f64();
    let failed_games = failures.len();
    let summary = Summary {
        schema_version: VERIFICATION_SCHEMA_VERSION,
        players,
        additional_action_space_enabled: extra,
        policy: policy.as_str(),
        seeds_requested: seeds,
        seeds_run,
        seed_start,
        shard,
        shards,
        passed_games,
        failed_games,
        elapsed_seconds: elapsed,
        mean_game_millis: if seeds_run == 0 {
            0.0
        } else {
            elapsed * 1000.0 / seeds_run as f64
        },
        checks,
        failures,
        commit_sha: std::env::var("GITHUB_SHA").unwrap_or_else(|_| "unknown".into()),
        binary_version: env!("CARGO_PKG_VERSION"),
    };
    let journal_path: PathBuf = out.join("games.jsonl");
    let summary_path: PathBuf = out.join("summary.json");
    if let Err(error) = std::fs::write(&journal_path, journal) {
        eprintln!("cannot write {}: {error}", journal_path.display());
        std::process::exit(1);
    }
    let summary_text = serde_json::to_string_pretty(&summary).expect("summary JSON");
    if let Err(error) = write_with_parent(&summary_path, &summary_text) {
        eprintln!("cannot write {}: {error}", summary_path.display());
        std::process::exit(1);
    }
    println!("{summary_text}");
    if summary.failed_games > 0 {
        eprintln!(
            "mode verification failed: {}/{} games failed for {} players, additional tile {}",
            summary.failed_games, summary.seeds_run, players, extra
        );
        std::process::exit(1);
    }
}

fn write_with_parent(path: &Path, content: &str) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, content)
}
