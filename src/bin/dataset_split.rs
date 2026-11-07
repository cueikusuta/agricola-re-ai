//! Build a train/validation/test split manifest and evaluate the M14 G0 gate
//! (#167 C).
//!
//! Two subcommands, both machine-readable and both fail closed:
//!
//! ```text
//! dataset_split manifest --in episodes.jsonl --out split-manifest.json \
//!     --train 0:499 --validation 500:649 --test 650:799
//! dataset_split g0 --manifest split-manifest.json --dataset-report report.json \
//!     [--evidence g0-evidence.json] --out g0-report.json
//! ```
//!
//! `manifest` exits non-zero when the split is unusable (seed outside a range,
//! mixed provenance, private leak, replay failure, empty split). `g0` writes a
//! verdict and exits 0 only for `pass`; `hold` and `fail` are non-zero so a
//! workflow cannot treat a missing-evidence gate as success.
use agricola_re_ai::dataset_split::{
    build_manifest, evaluate_g0, DatasetSplitManifest, G0Evidence, GateStatus, ModeBundle,
    SplitPlan,
};
use agricola_re_ai::episode_data::{from_jsonl, DatasetReport};
use serde_json::json;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

fn main() -> ExitCode {
    match run() {
        Ok(code) => code,
        Err(error) => {
            eprintln!("dataset_split failed: {error}");
            ExitCode::from(2)
        }
    }
}

fn run() -> Result<ExitCode, String> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    let command = args
        .first()
        .ok_or("usage: dataset_split manifest|g0 [options]")?;
    let mut values = BTreeMap::new();
    let mut iter = args[1..].iter();
    while let Some(flag) = iter.next() {
        let key = flag
            .strip_prefix("--")
            .ok_or_else(|| format!("unexpected argument: {flag}"))?;
        let value = iter
            .next()
            .ok_or_else(|| format!("missing value for --{key}"))?;
        if values.insert(key.to_string(), value.to_string()).is_some() {
            return Err(format!("duplicate option: --{key}"));
        }
    }
    match command.as_str() {
        "manifest" => build_from_episodes(&values),
        "g0" => evaluate(&values),
        other => Err(format!("unknown subcommand: {other}")),
    }
}

fn required<'a>(values: &'a BTreeMap<String, String>, key: &str) -> Result<&'a str, String> {
    values
        .get(key)
        .map(String::as_str)
        .ok_or_else(|| format!("missing --{key}"))
}

fn parse_range(value: &str, key: &str) -> Result<(u64, u64), String> {
    let (low, high) = value
        .split_once(':')
        .ok_or_else(|| format!("--{key} must be LOW:HIGH (inclusive)"))?;
    let low = low
        .parse::<u64>()
        .map_err(|_| format!("invalid --{key} lower bound"))?;
    let high = high
        .parse::<u64>()
        .map_err(|_| format!("invalid --{key} upper bound"))?;
    if low > high {
        return Err(format!(
            "--{key} lower bound must not exceed its upper bound"
        ));
    }
    Ok((low, high))
}

fn build_from_episodes(values: &BTreeMap<String, String>) -> Result<ExitCode, String> {
    let input = required(values, "in")?;
    let output = required(values, "out")?;
    let text = fs::read_to_string(input).map_err(|error| format!("{input}: {error}"))?;
    let records = from_jsonl(&text)?;
    let plan = SplitPlan {
        train: parse_range(required(values, "train")?, "train")?,
        validation: parse_range(required(values, "validation")?, "validation")?,
        test: parse_range(required(values, "test")?, "test")?,
    };
    let manifest = build_manifest(&records, plan)?;
    fs::write(
        output,
        serde_json::to_string_pretty(&manifest).map_err(|error| error.to_string())? + "\n",
    )
    .map_err(|error| format!("{output}: {error}"))?;
    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "manifest": output,
            "usable": manifest.is_usable(),
            "episodes": manifest.total_episodes,
            "training_samples": manifest.total_training_samples,
            "splits": manifest
                .splits
                .iter()
                .map(|entry| json!({
                    "split": entry.split.as_str(),
                    "episodes": entry.episodes,
                    "seeds": entry.seeds.len(),
                    "training_samples": entry.training_samples,
                }))
                .collect::<Vec<_>>(),
            "provenance": manifest.provenance,
        }))
        .map_err(|error| error.to_string())?
    );
    Ok(ExitCode::SUCCESS)
}

fn evaluate(values: &BTreeMap<String, String>) -> Result<ExitCode, String> {
    let manifest_path = required(values, "manifest")?;
    let report_path = required(values, "dataset-report")?;
    let output = required(values, "out")?;
    let manifest: DatasetSplitManifest = serde_json::from_str(
        &fs::read_to_string(manifest_path).map_err(|error| format!("{manifest_path}: {error}"))?,
    )
    .map_err(|error| format!("{manifest_path}: {error}"))?;
    let dataset: DatasetReport = serde_json::from_str(
        &fs::read_to_string(report_path).map_err(|error| format!("{report_path}: {error}"))?,
    )
    .map_err(|error| format!("{report_path}: {error}"))?;
    let evidence = match values.get("evidence") {
        Some(path) => {
            let text = fs::read_to_string(path).map_err(|error| format!("{path}: {error}"))?;
            let evidence: G0Evidence =
                serde_json::from_str(&text).map_err(|error| format!("{path}: {error}"))?;
            Some((evidence, PathBuf::from(path)))
        }
        None => None,
    };
    // The gate reads the artifacts the evidence points at, so nothing it checks
    // is a hand-typed number. Paths are resolved relative to the evidence file.
    let mut bundles: BTreeMap<String, ModeBundle> = BTreeMap::new();
    if let Some((evidence, evidence_path)) = &evidence {
        let base = evidence_path.parent().unwrap_or(Path::new("."));
        for mode in &evidence.modes {
            let mut bundle = ModeBundle::default();
            for declared in &mode.matches_paths {
                match fs::read(base.join(declared)) {
                    Ok(bytes) => bundle.matches.push(bytes),
                    Err(error) => {
                        bundle.load_error = Some(format!("{declared}: {error}"));
                        break;
                    }
                }
            }
            if let Some(checks) = &mode.checks_path {
                match fs::read(base.join(checks)) {
                    Ok(bytes) => bundle.checks = Some(bytes),
                    Err(error) => {
                        bundle.load_error = Some(format!("{checks}: {error}"));
                    }
                }
            }
            bundles.insert(mode.mode.clone(), bundle);
        }
    }
    let report = evaluate_g0(
        &manifest,
        &dataset,
        evidence.as_ref().map(|(evidence, _)| evidence),
        &bundles,
    );
    fs::write(
        output,
        serde_json::to_string_pretty(&report).map_err(|error| error.to_string())? + "\n",
    )
    .map_err(|error| format!("{output}: {error}"))?;
    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "verdict": report.verdict.as_str(),
            "report": output,
            "counts": report.counts(),
            "next_action": report.next_action,
        }))
        .map_err(|error| error.to_string())?
    );
    Ok(match report.verdict {
        GateStatus::Pass => ExitCode::SUCCESS,
        GateStatus::Hold | GateStatus::Fail => ExitCode::FAILURE,
    })
}
