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
    read_manifest_named(dir, "run-manifest.json")
}

fn read_manifest_named(dir: &Path, name: &str) -> serde_json::Value {
    serde_json::from_str(&fs::read_to_string(dir.join(name)).unwrap()).unwrap()
}

fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
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
    assert_eq!(manifest["schema_version"], 2);
    assert_eq!(manifest["run_mode"], "smoke");
    assert_eq!(manifest["status"], "finished");
    assert_eq!(manifest["players"], 4);
    assert_eq!(manifest["cards"], "none");
    assert_eq!(manifest["official_final_mode"], false);
    assert_eq!(manifest["official_pool_ready"], false);
    assert_eq!(manifest["pool_gate_snapshot"]["g1_met"], false);
    if manifest["code_sha"] == "unknown" {
        assert_eq!(
            manifest["pool_gate_snapshot"]["measured_at_sha"],
            serde_json::Value::Null,
            "a smoke run without a code SHA must not pretend to be measured at one"
        );
    } else {
        assert_eq!(
            manifest["pool_gate_snapshot"]["measured_at_sha"], manifest["code_sha"],
            "the pool gate must be measured at the run's code identity"
        );
    }
    assert_eq!(manifest["strength_claim_allowed"], false);
    assert_eq!(
        manifest["comparison_design"]["head_to_head"], true,
        "opponent defaults to the baseline, so this is head-to-head"
    );
    assert_eq!(
        manifest["condition_id"].as_str().unwrap().len(),
        64,
        "condition id must be a sha256 hex digest"
    );
    let pre = read_manifest_named(&dir, "run-manifest.pre.json");
    assert_eq!(pre["status"], "running");
    assert_eq!(pre["condition_id"], manifest["condition_id"]);
    assert_eq!(
        pre["preregistration"], manifest["preregistration"],
        "the pre-registration must not change after the games"
    );
    let pre_bytes = std::fs::read(dir.join("run-manifest.pre.json")).unwrap();
    let digest = sha256_hex(&pre_bytes);
    assert_eq!(
        digest,
        manifest["preregistration_sha256"].as_str().unwrap(),
        "the final manifest must bind the pre-registration bytes"
    );

    let rows = read_rows(&dir);
    assert_eq!(rows.len(), 8, "one block = 4 seats x 2 arms");
    for row in rows.iter().filter(|row| row["arm_id"] == "candidate") {
        let ids = row["game_policy_id_by_seat"].as_array().unwrap();
        assert_eq!(
            ids.iter().filter(|id| **id == "eval_veg").count(),
            1,
            "a candidate arm has exactly one candidate seat"
        );
        assert_eq!(ids.iter().filter(|id| **id == "greedy").count(), 3);
    }
    for row in rows.iter().filter(|row| row["arm_id"] == "baseline") {
        let ids = row["game_policy_id_by_seat"].as_array().unwrap();
        assert!(
            ids.iter().all(|id| *id == "greedy"),
            "the baseline arm is the all-baseline mirror"
        );
    }
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
        assert_eq!(row["rounds_completed"], 14);
        assert_eq!(row["interference_proxy"], serde_json::Value::Null);
        assert_eq!(row["pick_illegal_fallback_count"], serde_json::Value::Null);
        assert!(row.get("pick_fallback_count").is_none());
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
        assert_eq!(row["pick_illegal_fallback_count"], 0);
        assert_eq!(row["draft_pick_budget_enforced"], false);
        assert_eq!(
            row["draft_actions"].as_array().unwrap().len(),
            28,
            "four seats x seven simultaneous picks"
        );
        assert_eq!(row["draft_event_count"], 28);
        assert_eq!(row["draft_state_hash"].as_str().unwrap().len(), 64);
    }
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn common_opponent_runs_keep_the_opponent_in_the_non_focus_seats() {
    let dir = unique_dir("common-opponent");
    let status = harness()
        .args([
            "--output",
            dir.to_str().unwrap(),
            "--players",
            "4",
            "--seed-start",
            "700120",
            "--blocks",
            "1",
            "--candidate",
            "eval_veg",
            "--baseline",
            "greedy",
            "--opponent",
            "eval",
            "--smoke",
        ])
        .status()
        .unwrap();
    assert!(status.success(), "harness failed");
    let manifest = read_manifest(&dir);
    assert_eq!(
        manifest["comparison_design"]["head_to_head"], false,
        "a common opponent is not head-to-head"
    );
    for row in read_rows(&dir) {
        assert_eq!(row["opponent_policy"], "eval");
        let policies = row["game_policy_id_by_seat"].as_array().unwrap();
        let seat = row["candidate_seat"].as_u64().unwrap() as usize;
        for (other, policy) in policies.iter().enumerate() {
            if other == seat {
                assert_eq!(
                    policy.as_str().unwrap(),
                    row["focus_policy"].as_str().unwrap()
                );
            } else {
                assert_eq!(policy.as_str().unwrap(), "eval");
            }
        }
    }
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn fixed_seed_reproduces_the_same_rows() {
    let run = |tag: &str| {
        let dir = unique_dir(tag);
        let status = harness()
            .args([
                "--output",
                dir.to_str().unwrap(),
                "--players",
                "4",
                "--seed-start",
                "700130",
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
        assert!(status.success());
        let rows = read_rows(&dir);
        let hashes: Vec<String> = rows
            .iter()
            .map(|row| row["replay_hash"].as_str().unwrap().to_string())
            .collect();
        let _ = fs::remove_dir_all(&dir);
        hashes
    };
    assert_eq!(run("repro-a"), run("repro-b"));
}

#[test]
fn block_and_draft_seed_arithmetic_reaches_every_row() {
    let dir = unique_dir("seeds");
    let status = harness()
        .args([
            "--output",
            dir.to_str().unwrap(),
            "--players",
            "4",
            "--seed-start",
            "700140",
            "--draft-seed-start",
            "710000",
            "--block-start",
            "2",
            "--blocks",
            "2",
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
    assert!(status.success());
    let rows = read_rows(&dir);
    let mut keys = std::collections::BTreeSet::new();
    for row in &rows {
        let block = row["block_index"].as_u64().unwrap();
        assert_eq!(row["seed"].as_u64().unwrap(), 700140 + block);
        assert_eq!(row["draft_seed"].as_u64().unwrap(), 710000 + block);
        keys.insert((
            row["condition_id"].as_str().unwrap().to_string(),
            block,
            row["arm_id"].as_str().unwrap().to_string(),
            row["candidate_seat"].as_u64().unwrap(),
        ));
    }
    assert_eq!(keys.len(), rows.len(), "block/arm/seat keys must be unique");
    assert_eq!(rows.len(), 16, "2 blocks x 4 seats x 2 arms");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn fixture_cards_without_the_draft_mode_are_rejected() {
    let dir = unique_dir("fixture-alone");
    let status = harness()
        .args([
            "--output",
            dir.to_str().unwrap(),
            "--players",
            "4",
            "--seed-start",
            "700150",
            "--blocks",
            "1",
            "--candidate",
            "eval",
            "--baseline",
            "greedy",
            "--cards",
            "fixture_4p",
            "--smoke",
        ])
        .status()
        .unwrap();
    assert!(!status.success());
    let _ = fs::remove_dir_all(&dir);
}
