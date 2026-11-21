//! End-to-end smoke checks for the design-aligned `eval_harness` binary.
//!
//! These runs are deliberately tiny (one seed block = 8 games at four players)
//! and are not measurements. They verify the artifact contract the aggregation
//! chapter consumes: the manifest, schema-2 rows, replay flags, draft fields,
//! and the refusal to reuse an output directory.
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

fn unique_dir(tag: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!(
        "agri-eval-harness-{tag}-{}-{nanos}",
        std::process::id()
    ))
}

fn harness() -> Command {
    Command::new(env!("CARGO_BIN_EXE_eval_harness"))
}

fn read_rows(dir: &Path) -> Vec<serde_json::Value> {
    let text = fs::read_to_string(dir.join("shard-0/matches.jsonl")).unwrap();
    text.lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}

fn read_manifest(dir: &Path) -> serde_json::Value {
    serde_json::from_str(&fs::read_to_string(dir.join("run-manifest.json")).unwrap()).unwrap()
}

#[test]
fn cardless_four_player_smoke_writes_schema_two_rows_and_a_manifest() {
    let dir = unique_dir("cardless");
    let status = harness()
        .args([
            "--output",
            dir.to_str().unwrap(),
            "--players",
            "4",
            "--seed-start",
            "700100",
            "--blocks",
            "1",
            "--candidate",
            "eval_veg",
            "--baseline",
            "greedy",
            "--smoke",
        ])
        .status()
        .unwrap();
    assert!(status.success(), "harness failed");

    let manifest = read_manifest(&dir);
    assert_eq!(manifest["run_mode"], "smoke");
    assert_eq!(manifest["players"], 4);
    assert_eq!(manifest["cards"], "none");
    assert_eq!(manifest["official_final_mode"], false);
    assert_eq!(manifest["strength_claim_allowed"], false);
    assert_eq!(
        manifest["condition_id"].as_str().unwrap().len(),
        64,
        "condition id must be a sha256 hex digest"
    );

    let rows = read_rows(&dir);
    assert_eq!(rows.len(), 8, "one block = 4 seats x 2 arms");
    for row in &rows {
        assert_eq!(row["schema_version"], 2);
        assert_eq!(row["finished"], true);
        assert_eq!(row["rounds_completed"], 14);
        assert_eq!(row["replay_verified"], true);
        assert_eq!(row["draft_mode"], "none");
        assert_eq!(row["players"], 4);
        let credit = row["first_place_credit"].as_f64().unwrap();
        assert!((0.0..=1.0).contains(&credit));
        assert!(row["ranks_by_seat"].as_array().unwrap().len() == 4);
        assert!(row["scores_by_seat"].as_array().unwrap().len() == 4);
    }

    // Fail closed: an existing run directory must not be appended to.
    let again = harness()
        .args([
            "--output",
            dir.to_str().unwrap(),
            "--players",
            "4",
            "--seed-start",
            "700100",
            "--blocks",
            "1",
            "--candidate",
            "eval_veg",
            "--baseline",
            "greedy",
            "--smoke",
        ])
        .status()
        .unwrap();
    assert!(!again.success(), "an existing run must be refused");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn draft_fixture_smoke_records_the_coordinator_only_draft_fields() {
    let dir = unique_dir("draft");
    let status = harness()
        .args([
            "--output",
            dir.to_str().unwrap(),
            "--players",
            "4",
            "--seed-start",
            "700110",
            "--blocks",
            "1",
            "--candidate",
            "eval",
            "--baseline",
            "greedy",
            "--draft-mode",
            "draft_10_7_simultaneous",
            "--cards",
            "fixture_4p",
            "--candidate-draft-ai",
            "synergy",
            "--baseline-draft-ai",
            "card_value",
            "--smoke",
        ])
        .status()
        .unwrap();
    assert!(status.success(), "harness failed");

    let manifest = read_manifest(&dir);
    assert_eq!(manifest["draft"]["mode"], "draft_10_7_simultaneous");
    assert_eq!(manifest["cards"], "fixture_4p");
    assert!(
        manifest["official_final_mode_blocked_by"]
            .as_array()
            .unwrap()
            .iter()
            .any(|value| value.as_str().unwrap().contains("test-fixture")),
        "fixture runs must state that they are functional only"
    );

    let rows = read_rows(&dir);
    assert_eq!(rows.len(), 8);
    for row in &rows {
        assert_eq!(row["draft_mode"], "draft_10_7_simultaneous");
        assert_eq!(row["card_set_count"], 80);
        assert_eq!(row["final_hands_by_seat"].as_array().unwrap().len(), 4);
        assert_eq!(row["draft_policy_id_by_seat"].as_array().unwrap().len(), 4);
        let start = row["start_player"].as_u64().unwrap();
        assert!(start < 4);
        let hash = row["draft_replay_hash"].as_str().unwrap();
        assert_eq!(hash.len(), 64);
        assert!(hash.chars().all(|c| c.is_ascii_hexdigit()));
        assert_eq!(row["pick_timeout_count"], 0);
        assert_eq!(row["pick_fallback_count"], 0);
    }
    let _ = fs::remove_dir_all(&dir);
}
