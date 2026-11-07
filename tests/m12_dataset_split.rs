//! #167 C: train/validation/test split manifest and the M14 G0 gate.
//!
//! These tests fix three things the roadmap asks for: the split cannot leak a
//! seed or an initial state across splits, G0 never reports `pass` while the
//! grounds are unproven, and — after the review P1 — **G0 never passes on a
//! declaration**. Every scale, replay and completeness number the gate uses is
//! recomputed from the matches files, so a hand-written `games_per_arm: 1000` or
//! a pair of `true` booleans cannot open the gate.
//!
//! Nothing here trains a model and nothing claims strength. The few-game cases
//! are explicitly plumbing tests.
use agricola_re_ai::cards::CardSet;
use agricola_re_ai::dataset_split::{
    build_manifest, evaluate_g0, load_matches, normalise_card_mode, recompute_mode, sha256_hex,
    ChecksEvidence, DatasetSplitManifest, G0Evidence, GateStatus, ModeBundle, ModeEvidence, Split,
    SplitPlan,
};
use agricola_re_ai::episode_data::{inspect_dataset, record_self_play, EpisodeMeta, EpisodeRecord};
use agricola_re_ai::evaluation::EvaluationPolicy;
use agricola_re_ai::model::Config;
use agricola_re_ai::policy::SimpleDetails;
use std::collections::BTreeMap;

fn meta(players: usize) -> EpisodeMeta {
    EpisodeMeta::new("test-sha", "none", vec!["eval".to_string(); players])
}

fn record(players: usize, seed: u64) -> EpisodeRecord {
    record_self_play(
        Config {
            players,
            seed,
            additional_action_space_enabled: false,
        },
        CardSet::default(),
        meta(players),
        &EvaluationPolicy,
        &SimpleDetails,
    )
    .expect("self-play recording")
}

/// Seeds 100..=101 train, 110 validation, and a wide locked test *range* so an
/// evaluation can use many unseen seeds without shipping that many episodes.
fn plan() -> SplitPlan {
    SplitPlan {
        train: (100, 101),
        validation: (110, 110),
        test: (120, 1600),
    }
}

fn dataset() -> Vec<EpisodeRecord> {
    [100, 101, 110, 120]
        .into_iter()
        .map(|seed| record(2, seed))
        .collect()
}

fn manifest() -> DatasetSplitManifest {
    build_manifest(&dataset(), plan()).expect("a clean split")
}

// --- Synthetic evaluation artifacts ----------------------------------------
//
// These are *rows*, not games: the gate only reads what a real `paired_eval`
// run writes, so constructing the rows is exactly what an artifact check needs.

fn row_json(block: usize, seed: u64, arm: &str, seat: usize, players: usize) -> String {
    format!(
        "{{\"block_index\":{block},\"seed\":{seed},\"arm_id\":\"{arm}\",\
         \"candidate_seat\":{seat},\"players\":{players},\"cards\":\"none\",\
         \"finished\":true,\"rounds_completed\":14,\"replay_verified\":true}}"
    )
}

/// A complete seat-rotated evaluation over `blocks` seed blocks.
fn matches_jsonl(blocks: usize, players: usize, first_seed: u64) -> String {
    let mut out = String::new();
    for block in 0..blocks {
        let seed = first_seed + block as u64;
        for arm in ["candidate", "baseline"] {
            for seat in 0..players {
                out.push_str(&row_json(block, seed, arm, seat, players));
                out.push('\n');
            }
        }
    }
    out
}

fn checks_json(code_sha: &str, matches_sha256: &str) -> String {
    serde_json::to_string(&ChecksEvidence {
        schema_version: 2,
        code_sha: code_sha.into(),
        matches_sha256: matches_sha256.into(),
        m8_rule_evidence: true,
        m8_legal_api_evidence: true,
        observation_noninterference_passed: true,
        ci_reproducible: true,
    })
    .expect("checks json")
}

/// Build evidence plus the bytes it points at for one mode.
fn evidence_for(
    mode: &str,
    players: usize,
    card_mode: &str,
    matches: &str,
    with_checks: bool,
) -> (G0Evidence, BTreeMap<String, ModeBundle>) {
    let digest = sha256_hex(matches.as_bytes());
    let mut bundle = ModeBundle {
        matches: vec![matches.as_bytes().to_vec()],
        checks: None,
        load_error: None,
    };
    let checks_sha = if with_checks {
        let checks = checks_json("test-sha", &digest);
        let sha = sha256_hex(checks.as_bytes());
        bundle.checks = Some(checks.into_bytes());
        Some(sha)
    } else {
        None
    };
    let evidence = G0Evidence {
        schema_version: 2,
        code_sha: "test-sha".into(),
        modes: vec![ModeEvidence {
            mode: mode.into(),
            players,
            card_mode: card_mode.into(),
            artifact_url: "https://example.invalid/run/1".into(),
            matches_sha256: digest,
            matches_paths: vec!["matches.jsonl".into()],
            checks_path: checks_sha.as_ref().map(|_| "checks.json".to_string()),
            checks_sha256: checks_sha,
        }],
    };
    let mut bundles = BTreeMap::new();
    bundles.insert(mode.to_string(), bundle);
    (evidence, bundles)
}

// --- Split manifest ---------------------------------------------------------

#[test]
fn a_disjoint_plan_produces_three_non_empty_splits() {
    let manifest = manifest();
    assert_eq!(manifest.total_episodes, 4);
    for split in [Split::Train, Split::Validation, Split::Test] {
        let contents = manifest.split(split).expect("split present");
        assert!(
            !contents.seeds.is_empty(),
            "{} must not be empty",
            split.as_str()
        );
        assert!(contents.training_samples > 0);
    }
    assert_eq!(manifest.split(Split::Train).unwrap().seeds, vec![100, 101]);
    assert_eq!(manifest.split(Split::Test).unwrap().seeds, vec![120]);
    assert_eq!(manifest.player_counts(), [2].into_iter().collect());
    assert!(manifest.seed_isolation_problems().is_empty());
    assert!(manifest.is_usable());
    let encoded = serde_json::to_string(&manifest).expect("serialise");
    let decoded: DatasetSplitManifest = serde_json::from_str(&encoded).expect("deserialise");
    assert_eq!(decoded, manifest);
}

#[test]
fn overlapping_ranges_are_rejected() {
    let plan = SplitPlan {
        train: (100, 110),
        validation: (110, 120),
        test: (130, 140),
    };
    let error = build_manifest(&dataset(), plan).unwrap_err();
    assert!(error.contains("overlap"), "unexpected error: {error}");
}

#[test]
fn a_seed_outside_every_range_is_rejected_instead_of_defaulting() {
    let plan = SplitPlan {
        train: (100, 101),
        validation: (110, 110),
        test: (200, 210),
    };
    let error = build_manifest(&dataset(), plan).unwrap_err();
    assert!(
        error.contains("outside every declared split range"),
        "unexpected error: {error}"
    );
}

#[test]
fn a_duplicate_seed_in_one_dataset_is_rejected() {
    let mut records = dataset();
    records.push(record(2, 100));
    let error = build_manifest(&records, plan()).unwrap_err();
    assert!(
        error.contains("more than once"),
        "unexpected error: {error}"
    );
}

#[test]
fn provenance_must_be_uniform_across_the_dataset() {
    let mut records = dataset();
    records[0].code_sha = "other-sha".into();
    let error = build_manifest(&records, plan()).unwrap_err();
    assert!(
        error.contains("mixed code_shas"),
        "unexpected error: {error}"
    );
}

#[test]
fn an_unverified_card_mode_is_rejected() {
    let mut records = dataset();
    for record in &mut records {
        record.card_mode = "experimental_pool".into();
    }
    let error = build_manifest(&records, plan()).unwrap_err();
    assert!(
        error.contains("not a verified condition"),
        "unexpected error: {error}"
    );
}

#[test]
fn a_corrupt_record_is_rejected_before_it_can_be_split() {
    let mut records = dataset();
    records[2].final_state_hash = "0".repeat(64);
    let error = build_manifest(&records, plan()).unwrap_err();
    assert!(error.contains("record 2"), "unexpected error: {error}");
}

#[test]
fn training_samples_never_carry_private_state() {
    let manifest = manifest();
    assert!(manifest.private_sample_leaks.is_empty());
    assert!(manifest.total_training_samples > 0);
    for record in dataset() {
        for viewer in 0..record.config.players {
            for sample in record.training_samples(viewer).unwrap() {
                sample.assert_public().expect("public sample");
                let json = serde_json::to_string(&sample).unwrap();
                for secret in ["action_seed", "hidden_round_deck", "events"] {
                    assert!(!json.contains(secret), "sample leaked {secret}");
                }
            }
        }
    }
}

#[test]
fn the_same_initial_state_in_two_splits_is_leakage() {
    let mut dataset = manifest();
    let shared = dataset
        .split(Split::Train)
        .unwrap()
        .initial_state_hashes
        .iter()
        .next()
        .cloned()
        .unwrap();
    let test_index = dataset
        .splits
        .iter()
        .position(|entry| entry.split == Split::Test)
        .unwrap();
    dataset.splits[test_index]
        .initial_state_hashes
        .insert(shared);
    let problems = dataset.initial_state_isolation_problems();
    assert_eq!(problems.len(), 1);
    assert!(
        problems[0].contains("shared by train and test"),
        "{problems:?}"
    );
}

#[test]
fn an_edited_manifest_with_a_seed_in_two_splits_is_detected() {
    let mut manifest = manifest();
    let test_seed = manifest.split(Split::Test).unwrap().seeds[0];
    let train_index = manifest
        .splits
        .iter()
        .position(|entry| entry.split == Split::Train)
        .unwrap();
    manifest.splits[train_index].seeds.push(test_seed);
    let problems = manifest.seed_isolation_problems();
    assert!(!problems.is_empty());
    assert!(
        problems[0].contains("appears in train and test"),
        "{problems:?}"
    );
}

// --- Recompute (the review P1 core) ----------------------------------------

#[test]
fn the_gate_recomputes_scale_from_rows_not_from_a_declaration() {
    // A two-block, two-player comparison is 8 rows: 2 blocks x 2 seats x 2 arms.
    let rows = load_matches(matches_jsonl(2, 2, 120).as_bytes()).expect("rows");
    let facts = recompute_mode(&rows);
    assert_eq!(facts.rows, 8);
    assert_eq!(facts.candidate_rows, 4);
    assert_eq!(facts.baseline_rows, 4);
    assert_eq!(facts.games_per_arm(), 4);
    assert_eq!(facts.expected_rows(), 8);
    assert!(facts.is_complete());
    // Every seed block legitimately holds one game per seat per arm.
    assert_eq!(facts.blocks.len(), 2);
    assert_eq!(facts.seats, [0usize, 1].into_iter().collect());
}

#[test]
fn a_block_missing_one_seat_is_incomplete() {
    let mut text = String::new();
    text.push_str(&row_json(0, 120, "candidate", 0, 2));
    text.push('\n');
    text.push_str(&row_json(0, 120, "candidate", 1, 2));
    text.push('\n');
    text.push_str(&row_json(0, 120, "baseline", 0, 2));
    text.push('\n');
    let rows = load_matches(text.as_bytes()).expect("rows");
    let facts = recompute_mode(&rows);
    assert!(!facts.is_complete());
    assert_eq!(facts.incomplete_blocks, vec![0]);
}

#[test]
fn duplicate_blocks_arms_and_seats_are_detected() {
    let mut text = matches_jsonl(1, 2, 120);
    text.push_str(&row_json(0, 120, "candidate", 0, 2));
    text.push('\n');
    let rows = load_matches(text.as_bytes()).expect("rows");
    let facts = recompute_mode(&rows);
    assert_eq!(facts.duplicate_keys, vec![(0, "candidate".to_string(), 0)]);
    assert!(!facts.is_complete());
}

#[test]
fn unverified_or_truncated_rows_are_detected() {
    let text = "{\"block_index\":0,\"seed\":120,\"arm_id\":\"candidate\",\
                \"candidate_seat\":0,\"players\":2,\"finished\":false,\
                \"rounds_completed\":13,\"replay_verified\":false}\n";
    let rows = load_matches(text.as_bytes()).expect("rows");
    let facts = recompute_mode(&rows);
    assert_eq!(facts.unfinished, 1);
    assert_eq!(facts.wrong_rounds, 1);
    assert_eq!(facts.unverified, 1);
    assert!(!facts.is_complete());
}

#[test]
fn a_truncated_artifact_line_is_an_error_not_a_smaller_run() {
    assert!(load_matches(b"{\"block_index\":0,").is_err());
    assert!(load_matches(b"  \n\n").is_err());
}

// --- G0 ---------------------------------------------------------------------

#[test]
fn g0_is_hold_not_pass_without_evidence() {
    let manifest = manifest();
    let report = evaluate_g0(
        &manifest,
        &inspect_dataset(&dataset()),
        None,
        &BTreeMap::new(),
    );
    assert_eq!(report.verdict, GateStatus::Hold);
    assert!(report.not_a_strength_claim);
    let (_, fails, holds) = report.counts();
    assert_eq!(fails, 0, "a missing evidence file is not a failure");
    assert!(holds > 0);
}

#[test]
fn the_old_declaration_only_evidence_schema_cannot_pass() {
    // The pre-review format had `games_per_arm`/booleans and no artifacts. It
    // must be rejected outright, not silently accepted as "v1 compatible".
    let legacy = serde_json::json!({
        "schema_version": 1,
        "code_sha": "test-sha",
        "m8_1_rule_evidence": true,
        "m8_2_legal_api_evidence": true,
        "modes": [{
            "mode": "cardless_2p", "players": 2, "card_mode": "none",
            "games_per_arm": 1000, "anomalies": 0, "replay_mismatches": 0,
            "legal_action_violations": 0,
            "observation_noninterference_passed": true,
            "ci_reproducible": true,
            "artifact_url": "https://example.invalid/run/1"
        }]
    });
    let evidence: G0Evidence = serde_json::from_value(legacy).expect("legacy shape parses");
    let report = evaluate_g0(
        &manifest(),
        &inspect_dataset(&dataset()),
        Some(&evidence),
        &BTreeMap::new(),
    );
    assert_eq!(report.verdict, GateStatus::Fail);
    assert!(
        report
            .checks
            .iter()
            .any(|check| check.id == "evidence_schema" && check.status == GateStatus::Fail),
        "checks: {:?}",
        report.checks
    );
}

#[test]
fn a_small_run_is_a_plumbing_test_and_cannot_pass() {
    let (evidence, bundles) =
        evidence_for("cardless_2p", 2, "none", &matches_jsonl(2, 2, 120), true);
    let report = evaluate_g0(
        &manifest(),
        &inspect_dataset(&dataset()),
        Some(&evidence),
        &bundles,
    );
    assert_eq!(report.verdict, GateStatus::Fail);
    let scale = report
        .checks
        .iter()
        .find(|check| check.id == "evaluation_scale")
        .expect("scale check");
    assert_eq!(scale.status, GateStatus::Fail);
    assert!(scale.detail.contains("recomputed"), "{}", scale.detail);
}

#[test]
fn a_thousand_games_per_arm_with_full_attestation_passes() {
    // 500 blocks x 2 seats x 2 arms = 1,000 games per arm. This is synthetic
    // *rows*, so it proves the gate's arithmetic and integrity chain, not that a
    // real evaluation happened.
    let matches = matches_jsonl(500, 2, 120);
    let (evidence, bundles) = evidence_for("cardless_2p", 2, "none", &matches, true);
    let report = evaluate_g0(
        &manifest(),
        &inspect_dataset(&dataset()),
        Some(&evidence),
        &bundles,
    );
    assert_eq!(
        report.verdict,
        GateStatus::Pass,
        "checks: {:?}",
        report.checks
    );
    assert_eq!(report.evaluated_modes, vec!["cardless_2p".to_string()]);
    let scale = report
        .checks
        .iter()
        .find(|check| check.id == "evaluation_scale")
        .expect("scale check");
    assert_eq!(scale.status, GateStatus::Pass);
    assert!(scale.detail.contains("1000 games/arm"), "{}", scale.detail);
}

#[test]
fn attestations_without_a_bound_checks_file_are_only_hold() {
    let matches = matches_jsonl(500, 2, 120);
    let (evidence, bundles) = evidence_for("cardless_2p", 2, "none", &matches, false);
    let report = evaluate_g0(
        &manifest(),
        &inspect_dataset(&dataset()),
        Some(&evidence),
        &bundles,
    );
    assert_eq!(
        report.verdict,
        GateStatus::Hold,
        "checks: {:?}",
        report.checks
    );
    for id in [
        "m8_rule_evidence",
        "m8_legal_api_evidence",
        "evaluation_observation_noninterference",
        "evaluation_ci_reproducible",
    ] {
        let check = report
            .checks
            .iter()
            .find(|check| check.id == id)
            .unwrap_or_else(|| panic!("{id} missing"));
        assert_eq!(check.status, GateStatus::Hold, "{id}: {}", check.detail);
    }
}

#[test]
fn a_checks_file_that_does_not_match_the_matches_digest_fails() {
    let matches = matches_jsonl(500, 2, 120);
    let (mut evidence, mut bundles) = evidence_for("cardless_2p", 2, "none", &matches, true);
    // Re-bind the checks file to different bytes: the binding must break.
    let other = matches_jsonl(500, 2, 900);
    let stale = checks_json("test-sha", &sha256_hex(other.as_bytes()));
    bundles.get_mut("cardless_2p").unwrap().checks = Some(stale.clone().into_bytes());
    evidence.modes[0].checks_sha256 = Some(sha256_hex(stale.as_bytes()));
    let report = evaluate_g0(
        &manifest(),
        &inspect_dataset(&dataset()),
        Some(&evidence),
        &bundles,
    );
    assert_eq!(report.verdict, GateStatus::Fail);
}

#[test]
fn a_fabricated_matches_digest_fails() {
    let matches = matches_jsonl(500, 2, 120);
    let (mut evidence, bundles) = evidence_for("cardless_2p", 2, "none", &matches, true);
    evidence.modes[0].matches_sha256 = sha256_hex(b"not the artifact");
    let report = evaluate_g0(
        &manifest(),
        &inspect_dataset(&dataset()),
        Some(&evidence),
        &bundles,
    );
    assert_eq!(report.verdict, GateStatus::Fail);
    assert!(report
        .checks
        .iter()
        .any(|check| check.id == "evaluation_matches_digest" && check.status == GateStatus::Fail));
}

#[test]
fn missing_matches_files_hold_instead_of_passing() {
    let matches = matches_jsonl(500, 2, 120);
    let (evidence, _) = evidence_for("cardless_2p", 2, "none", &matches, true);
    let report = evaluate_g0(
        &manifest(),
        &inspect_dataset(&dataset()),
        Some(&evidence),
        &BTreeMap::new(),
    );
    assert_eq!(report.verdict, GateStatus::Hold);
}

#[test]
fn duplicate_modes_are_rejected() {
    let matches = matches_jsonl(500, 2, 120);
    let (mut evidence, bundles) = evidence_for("cardless_2p", 2, "none", &matches, true);
    evidence.modes.push(evidence.modes[0].clone());
    let report = evaluate_g0(
        &manifest(),
        &inspect_dataset(&dataset()),
        Some(&evidence),
        &bundles,
    );
    assert_eq!(report.verdict, GateStatus::Fail);
    assert!(report
        .checks
        .iter()
        .any(|check| check.detail.contains("duplicate modes")));
}

#[test]
fn a_mode_condition_that_does_not_match_the_dataset_is_rejected() {
    // The dataset is cardless 2p; a verified-card 4p evaluation is not the same
    // experiment and must not be counted as evidence for it.
    let matches = matches_jsonl(500, 4, 120);
    let (evidence, bundles) = evidence_for("verified_4p", 4, "verified_basic", &matches, true);
    let report = evaluate_g0(
        &manifest(),
        &inspect_dataset(&dataset()),
        Some(&evidence),
        &bundles,
    );
    assert_eq!(report.verdict, GateStatus::Fail);
}

#[test]
fn an_unverified_card_mode_in_the_evidence_is_rejected() {
    let matches = matches_jsonl(500, 2, 120);
    let (evidence, bundles) = evidence_for("experimental", 2, "experimental_pool", &matches, true);
    let report = evaluate_g0(
        &manifest(),
        &inspect_dataset(&dataset()),
        Some(&evidence),
        &bundles,
    );
    assert_eq!(report.verdict, GateStatus::Fail);
}

#[test]
fn evaluating_on_training_seeds_is_leakage_and_fails() {
    // Seeds 100/101 are the train split; an evaluation must use the locked test
    // range only.
    let matches = matches_jsonl(500, 2, 100);
    let (evidence, bundles) = evidence_for("cardless_2p", 2, "none", &matches, true);
    let report = evaluate_g0(
        &manifest(),
        &inspect_dataset(&dataset()),
        Some(&evidence),
        &bundles,
    );
    assert_eq!(report.verdict, GateStatus::Fail);
    let split_check = report
        .checks
        .iter()
        .find(|check| check.id == "evaluation_split_discipline")
        .expect("split check");
    assert_eq!(split_check.status, GateStatus::Fail);
}

#[test]
fn an_unfinished_or_unreplayed_evaluation_fails() {
    let mut text = matches_jsonl(500, 2, 120);
    text = text.replace("\"replay_verified\":true", "\"replay_verified\":false");
    // Only the first occurrence is enough to prove the check.
    let mut changed = String::new();
    let mut replaced = false;
    for line in text.lines() {
        if !replaced && line.contains("\"replay_verified\":false") {
            changed.push_str(line);
            changed.push('\n');
            replaced = true;
        } else {
            changed
                .push_str(&line.replace("\"replay_verified\":false", "\"replay_verified\":true"));
            changed.push('\n');
        }
    }
    let (evidence, bundles) = evidence_for("cardless_2p", 2, "none", &changed, true);
    let report = evaluate_g0(
        &manifest(),
        &inspect_dataset(&dataset()),
        Some(&evidence),
        &bundles,
    );
    assert_eq!(report.verdict, GateStatus::Fail);
    assert!(report
        .checks
        .iter()
        .any(|check| check.id == "evaluation_replay" && check.status == GateStatus::Fail));
}

#[test]
fn g0_fails_a_dataset_with_a_corrupt_record() {
    let manifest = manifest();
    let mut records = dataset();
    records[0].final_state_hash = "0".repeat(64);
    let matches = matches_jsonl(500, 2, 120);
    let (evidence, bundles) = evidence_for("cardless_2p", 2, "none", &matches, true);
    let report = evaluate_g0(
        &manifest,
        &inspect_dataset(&records),
        Some(&evidence),
        &bundles,
    );
    assert_eq!(report.verdict, GateStatus::Fail);
    assert!(report
        .checks
        .iter()
        .any(|check| check.id == "dataset_integrity" && check.status == GateStatus::Fail));
}

#[test]
fn card_mode_spellings_are_normalised_consistently() {
    assert_eq!(normalise_card_mode("none"), "none");
    assert_eq!(normalise_card_mode("verified_basic_2p"), "verified_basic");
    assert_eq!(normalise_card_mode("verified"), "verified_basic");
}

// --- Seat rotation and seed-block integrity (#170 re-review) -----------------

/// A seat-rotated run where the caller controls the seed of every block.
fn matches_with_seeds(blocks: usize, players: usize, seed_of: impl Fn(usize) -> u64) -> String {
    let mut out = String::new();
    for block in 0..blocks {
        let seed = seed_of(block);
        for arm in ["candidate", "baseline"] {
            for seat in 0..players {
                out.push_str(&row_json(block, seed, arm, seat, players));
                out.push('\n');
            }
        }
    }
    out
}

/// The gate ties a mode to the dataset's own condition, so a 4-player
/// evaluation needs a 4-player dataset to be judged against.
fn dataset_for(players: usize) -> Vec<EpisodeRecord> {
    if players == 2 {
        return dataset();
    }
    [100, 101, 110, 120]
        .into_iter()
        .map(|seed| record(players, seed))
        .collect()
}

fn manifest_for(players: usize) -> DatasetSplitManifest {
    build_manifest(&dataset_for(players), plan()).expect("a clean split")
}

fn g0_with(matches: &str, players: usize) -> (GateStatus, Vec<String>) {
    let (evidence, bundles) = evidence_for("cardless_2p", players, "none", matches, true);
    let records = dataset_for(players);
    let report = evaluate_g0(
        &manifest_for(players),
        &inspect_dataset(&records),
        Some(&evidence),
        &bundles,
    );
    let failed = report
        .checks
        .iter()
        .filter(|check| check.status != GateStatus::Pass)
        .map(|check| check.id.clone())
        .collect();
    (report.verdict, failed)
}

#[test]
fn a_two_player_run_using_only_seat_zero_is_not_a_full_rotation() {
    // The reviewer's counter-example: 1,000 blocks x 2 rows with only seat 0
    // reaches "1000 games/arm" arithmetic but never rotated seats at all.
    let mut text = String::new();
    for block in 0..1000u64 {
        let seed = 120 + block;
        for arm in ["candidate", "baseline"] {
            text.push_str(&row_json(block as usize, seed, arm, 0, 2));
            text.push('\n');
        }
    }
    let facts = recompute_mode(&load_matches(text.as_bytes()).expect("rows"));
    // 1,000 blocks x 1 seat per arm reaches "1000 games/arm" exactly, so the
    // scale check alone would pass; only the seat-rotation check catches it.
    assert_eq!(facts.games_per_arm(), 1000);
    assert!(!facts.is_complete());
    assert!(!facts.incomplete_seat_coverage.is_empty());
    let (verdict, failed) = g0_with(&text, 2);
    assert_eq!(verdict, GateStatus::Fail, "failed checks: {failed:?}");
    assert!(failed.contains(&"evaluation_block_completeness".to_string()));
    assert!(
        !failed.contains(&"evaluation_scale".to_string()),
        "the scale check must not be what rejects this run: {failed:?}"
    );
}

#[test]
fn a_four_player_run_needs_all_eight_rows_per_block() {
    // 250 blocks x 4 seats x 2 arms = 1,000 games/arm and 8 rows per block.
    let complete = matches_with_seeds(250, 4, |block| 200 + block as u64);
    assert_eq!(g0_with(&complete, 4).0, GateStatus::Pass);

    // Drop one seat from one block: the run is no longer a full rotation.
    let mut missing = String::new();
    for block in 0..250usize {
        let seed = 200 + block as u64;
        for arm in ["candidate", "baseline"] {
            for seat in 0..4usize {
                if block == 7 && arm == "candidate" && seat == 3 {
                    continue;
                }
                missing.push_str(&row_json(block, seed, arm, seat, 4));
                missing.push('\n');
            }
        }
    }
    let facts = recompute_mode(&load_matches(missing.as_bytes()).expect("rows"));
    assert!(!facts.is_complete());
    assert_eq!(facts.incomplete_blocks, vec![7]);
    assert_eq!(g0_with(&missing, 4).0, GateStatus::Fail);
}

#[test]
fn a_seat_outside_the_player_range_is_rejected() {
    let mut text = matches_with_seeds(250, 4, |block| 200 + block as u64);
    text.push_str(&row_json(0, 200, "candidate", 4, 4));
    text.push('\n');
    let facts = recompute_mode(&load_matches(text.as_bytes()).expect("rows"));
    assert_eq!(facts.out_of_range_seats, [4usize].into_iter().collect());
    assert!(!facts.is_complete());
    assert_eq!(g0_with(&text, 4).0, GateStatus::Fail);
}

#[test]
fn a_reused_seed_is_not_a_thousand_independent_games() {
    // Every block claims the same seed: the arithmetic still says 1,000/arm, but
    // it is one seed seen 500 times.
    let reused = matches_with_seeds(500, 2, |_| 120);
    let facts = recompute_mode(&load_matches(reused.as_bytes()).expect("rows"));
    assert_eq!(facts.games_per_arm(), 1000);
    assert_eq!(facts.seeds.len(), 1);
    assert_eq!(facts.seeds_in_multiple_blocks, vec![120]);
    assert!(!facts.is_complete());
    let (verdict, failed) = g0_with(&reused, 2);
    assert_eq!(verdict, GateStatus::Fail, "failed checks: {failed:?}");
    assert!(failed.contains(&"evaluation_seed_blocks".to_string()));
}

#[test]
fn one_block_with_two_different_seeds_is_rejected() {
    let mut text = String::new();
    for arm in ["candidate", "baseline"] {
        for seat in 0..2usize {
            // Same block, different seed per arm.
            let seed = if arm == "candidate" { 120 } else { 999 };
            text.push_str(&row_json(0, seed, arm, seat, 2));
            text.push('\n');
        }
    }
    let facts = recompute_mode(&load_matches(text.as_bytes()).expect("rows"));
    assert_eq!(facts.blocks_with_mixed_seeds, vec![0]);
    assert!(!facts.is_complete());
    assert_eq!(g0_with(&text, 2).0, GateStatus::Fail);
}

#[test]
fn the_seed_block_offset_must_be_constant() {
    // seed = seed_start + block is the harness contract; a run whose offsets
    // disagree cannot be reproduced from seed_start alone.
    let mut text = matches_with_seeds(500, 2, |block| 120 + block as u64);
    text.push_str(&row_json(0, 777, "candidate", 0, 2));
    text.push('\n');
    let facts = recompute_mode(&load_matches(text.as_bytes()).expect("rows"));
    assert!(facts.seed_offset_conflicts >= 1);
    assert!(!facts.is_complete());
    assert_eq!(g0_with(&text, 2).0, GateStatus::Fail);
}

#[test]
fn a_correctly_rotated_run_passes_the_seat_and_seed_checks() {
    // 500 blocks x 2 seats x 2 arms with unique seeds inside the test range.
    let matches = matches_with_seeds(500, 2, |block| 120 + block as u64);
    let facts = recompute_mode(&load_matches(matches.as_bytes()).expect("rows"));
    assert!(facts.is_complete(), "{facts:?}");
    assert_eq!(facts.expected_rows(), 2000);
    assert_eq!(facts.games_per_arm(), 1000);
    assert_eq!(facts.seed_offsets, [120i64].into_iter().collect());
    let (verdict, failed) = g0_with(&matches, 2);
    assert_eq!(verdict, GateStatus::Pass, "failed checks: {failed:?}");
}

#[test]
fn a_checks_file_with_an_unsupported_schema_is_rejected() {
    let matches = matches_with_seeds(500, 2, |block| 120 + block as u64);
    let (mut evidence, mut bundles) = evidence_for("cardless_2p", 2, "none", &matches, true);
    let digest = sha256_hex(matches.as_bytes());
    let stale_checks = serde_json::to_string(&ChecksEvidence {
        schema_version: 1,
        code_sha: "test-sha".into(),
        matches_sha256: digest,
        m8_rule_evidence: true,
        m8_legal_api_evidence: true,
        observation_noninterference_passed: true,
        ci_reproducible: true,
    })
    .expect("checks json");
    bundles.get_mut("cardless_2p").unwrap().checks = Some(stale_checks.clone().into_bytes());
    evidence.modes[0].checks_sha256 = Some(sha256_hex(stale_checks.as_bytes()));
    let report = evaluate_g0(
        &manifest(),
        &inspect_dataset(&dataset()),
        Some(&evidence),
        &bundles,
    );
    assert_eq!(report.verdict, GateStatus::Fail);
    let attested = report
        .checks
        .iter()
        .find(|check| check.id == "checks_attested")
        .expect("checks_attested check");
    assert_eq!(attested.status, GateStatus::Fail);
    assert!(
        attested.detail.contains("checks schema 1 is not 2"),
        "{}",
        attested.detail
    );
}

#[test]
fn a_checks_file_with_the_supported_schema_passes() {
    let matches = matches_with_seeds(500, 2, |block| 120 + block as u64);
    let (verdict, failed) = g0_with(&matches, 2);
    assert_eq!(verdict, GateStatus::Pass, "failed checks: {failed:?}");
}
