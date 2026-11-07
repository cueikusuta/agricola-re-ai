//! Tests for the AI outcome diagnostics binary (Issue #106).
//!
//! These tests pin the properties the analysis depends on:
//! 1. The reported games are exactly the canonical `play_to_end` games, so the
//!    recorded metrics describe what the rest of the project plays.
//! 2. The log-derived family-growth rounds are well formed and consistent with
//!    the final family sizes (i.e. the prefix replay actually observed growth).
//! 3. The same seed/config produces byte-identical output, so a reported number
//!    can always be re-run.
use agricola_re_ai::evaluation::EvaluationPolicy;
use agricola_re_ai::policy::{play_to_end, SimpleDetails};
use agricola_re_ai::{Config, Game};
use serde_json::Value;
use std::path::PathBuf;
use std::process::Command;

fn workspace_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/ai-diag-tests")
}

fn run_diagnostics(tag: &str, players: usize, seeds: usize, seed_start: u64) -> Vec<Value> {
    let dir = workspace_dir();
    std::fs::create_dir_all(&dir).expect("create test output dir");
    let output = dir.join(format!("{tag}.jsonl"));
    let result = Command::new(env!("CARGO_BIN_EXE_ai_diagnostics"))
        .args([
            "--players",
            &players.to_string(),
            "--seeds",
            &seeds.to_string(),
            "--seed-start",
            &seed_start.to_string(),
            "--policy",
            "eval",
            "--additional-action-space",
            "false",
            "--output",
            output.to_str().unwrap(),
        ])
        .output()
        .expect("run ai_diagnostics");
    assert!(
        result.status.success(),
        "diagnostics failed: {}",
        String::from_utf8_lossy(&result.stderr)
    );
    let text = std::fs::read_to_string(&output).expect("read diagnostics output");
    text.lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str(line).expect("valid JSONL row"))
        .collect()
}

#[test]
fn matches_canonical_play_to_end_state_hash() {
    let seed_start = 940_000;
    let rows = run_diagnostics("equivalence", 4, 3, seed_start);
    assert_eq!(rows.len(), 3);

    for (offset, row) in rows.iter().enumerate() {
        let config = Config {
            players: 4,
            seed: seed_start + offset as u64,
            additional_action_space_enabled: false,
        };
        let mut game = Game::new(config);
        play_to_end(&mut game, &EvaluationPolicy, &SimpleDetails).unwrap();
        assert!(game.public.finished);
        assert_eq!(
            row["state_hash"].as_str().unwrap(),
            game.state_hash(),
            "seed {} diverged from canonical play_to_end",
            seed_start + offset as u64
        );
        assert_eq!(row["ranks"].as_array().unwrap().len(), 4);
        assert_eq!(row["metrics"].as_array().unwrap().len(), 4);
    }
}

/// The growth rounds come from canonical prefix replay, not from a parallel
/// decision loop, so this guards the derivation and its consistency with the
/// final family size for every player count.
#[test]
fn derived_growth_rounds_are_well_formed() {
    let mut saw_a_growth = false;
    for players in [2usize, 3, 4] {
        let rows = run_diagnostics(&format!("growth-{players}p"), players, 4, 960_000);
        for row in &rows {
            let metrics = row["metrics"].as_array().unwrap();
            assert_eq!(metrics.len(), players);
            assert_eq!(row["ranks"].as_array().unwrap().len(), players);
            for metric in metrics {
                let round = |key: &str| metric[key].as_u64();
                let family = metric["family_final"].as_u64().unwrap();
                for (threshold, key) in [
                    (3u64, "grow_third_round"),
                    (4, "grow_fourth_round"),
                    (5, "grow_fifth_round"),
                ] {
                    match round(key) {
                        Some(value) => {
                            assert!((1..=14).contains(&value), "{key}={value} out of range");
                            assert!(
                                family >= threshold,
                                "{key} recorded but final family {family} < {threshold}"
                            );
                            saw_a_growth = true;
                        }
                        None => assert!(
                            family < threshold,
                            "final family {family} >= {threshold} but {key} missing"
                        ),
                    }
                }
                let third = round("grow_third_round");
                let fourth = round("grow_fourth_round");
                let fifth = round("grow_fifth_round");
                if let (Some(third), Some(fourth)) = (third, fourth) {
                    assert!(third <= fourth, "third worker grew after fourth");
                }
                if let (Some(fourth), Some(fifth)) = (fourth, fifth) {
                    assert!(fourth <= fifth, "fourth worker grew after fifth");
                }
            }
        }
    }
    assert!(saw_a_growth, "no family growth was observed in the sample");
}

#[test]
fn same_seed_is_byte_for_byte_reproducible() {
    let first = run_diagnostics("repro-a", 3, 5, 950_000);
    let first_bytes = std::fs::read(workspace_dir().join("repro-a.jsonl")).unwrap();
    let second = run_diagnostics("repro-b", 3, 5, 950_000);
    let second_bytes = std::fs::read(workspace_dir().join("repro-b.jsonl")).unwrap();
    assert_eq!(first, second);
    assert_eq!(first_bytes, second_bytes, "same seed changed JSONL bytes");
    // A different seed range must not silently reuse cached rows.
    let other = run_diagnostics("repro-c", 3, 5, 951_000);
    let other_bytes = std::fs::read(workspace_dir().join("repro-c.jsonl")).unwrap();
    assert_ne!(first, other);
    assert_ne!(first_bytes, other_bytes);
}

#[test]
fn rejects_unknown_policy() {
    let dir = workspace_dir();
    std::fs::create_dir_all(&dir).expect("create test output dir");
    let output = dir.join("invalid.jsonl");
    let result = Command::new(env!("CARGO_BIN_EXE_ai_diagnostics"))
        .args([
            "--players",
            "2",
            "--seeds",
            "1",
            "--seed-start",
            "1",
            "--policy",
            "bogus",
            "--additional-action-space",
            "false",
            "--output",
            output.to_str().unwrap(),
        ])
        .output()
        .expect("run ai_diagnostics");
    assert!(!result.status.success());
}
