//! Inspect an episode-dataset JSONL artifact.
//!
//! This is the verification half of the M12 data pipeline: the shard job writes
//! JSONL with `episode_export`, then runs this binary so every record is parsed,
//! replay-verified and summarised as machine-readable JSON before the artifact
//! is uploaded. The aggregation job only trusts these reports, so a corrupt or
//! incomplete shard cannot be aggregated as success.
//!
//! Usage:
//!   episode_inspect --in episodes.jsonl
//!   episode_inspect --in episodes.jsonl --report report.json
//!
//! Exit status is 0 only when the whole set is clean, complete, and produced
//! under one uniform provenance. The report is written even on failure so CI
//! preserves the reason.

use agricola_re_ai::episode_data::{from_jsonl, inspect_dataset, DatasetReport};

struct Options {
    input: String,
    report: Option<String>,
}

fn parse() -> Result<Options, String> {
    let mut options = Options {
        input: String::new(),
        report: None,
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
            "--in" => options.input = value(index)?,
            "--report" => options.report = Some(value(index)?),
            other => return Err(format!("unknown argument {other}")),
        }
        index += 2;
    }
    if options.input.is_empty() {
        return Err("--in <episodes.jsonl> is required".into());
    }
    Ok(options)
}

fn verdict(report: &DatasetReport) -> bool {
    report.is_clean() && report.is_complete() && report.has_uniform_provenance()
}

fn main() {
    let options = match parse() {
        Ok(options) => options,
        Err(error) => {
            eprintln!("episode_inspect: {error}");
            std::process::exit(2);
        }
    };
    let text = match std::fs::read_to_string(&options.input) {
        Ok(text) => text,
        Err(error) => {
            eprintln!("episode_inspect: cannot read {}: {error}", options.input);
            std::process::exit(2);
        }
    };
    let records = match from_jsonl(&text) {
        Ok(records) => records,
        Err(error) => {
            eprintln!(
                "episode_inspect: {} is not valid JSONL: {error}",
                options.input
            );
            std::process::exit(2);
        }
    };
    let report = inspect_dataset(&records);
    let json = match serde_json::to_string_pretty(&report) {
        Ok(json) => json,
        Err(error) => {
            eprintln!("episode_inspect: cannot serialize report: {error}");
            std::process::exit(2);
        }
    };
    if let Some(path) = &options.report {
        if let Err(error) = std::fs::write(path, format!("{json}\n")) {
            eprintln!("episode_inspect: cannot write {path}: {error}");
            std::process::exit(2);
        }
    }
    println!("{json}");
    if !verdict(&report) {
        std::process::exit(1);
    }
}
