//! #167 C: train/validation/test split manifest and the M14 G0 gate.
//!
//! These tests fix the two things the roadmap's data-split section asks for:
//! the split cannot leak a seed or an initial state across splits, and G0 never
//! reports `pass` while evidence is missing. They do not train anything and do
//! not claim strength.
use agricola_re_ai::cards::CardSet;
use agricola_re_ai::dataset_split::{
    build_manifest, evaluate_g0, DatasetSplitManifest, G0Evidence, GateStatus, ModeEvidence, Split,
    SplitPlan,
};
use agricola_re_ai::episode_data::{inspect_dataset, record_self_play, EpisodeMeta, EpisodeRecord};
use agricola_re_ai::evaluation::EvaluationPolicy;
use agricola_re_ai::model::Config;
use agricola_re_ai::policy::SimpleDetails;

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

/// Seeds 100..=101 train, 110 validation, 120 test. Kept small: these tests are
/// about the split rules, and every record is a full self-play game.
fn plan() -> SplitPlan {
    SplitPlan {
        train: (100, 101),
        validation: (110, 110),
        test: (120, 120),
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

fn evidence(code_sha: &str) -> G0Evidence {
    G0Evidence {
        schema_version: 1,
        code_sha: code_sha.into(),
        m8_1_rule_evidence: true,
        m8_2_legal_api_evidence: true,
        modes: vec![ModeEvidence {
            mode: "cardless_2p".into(),
            players: 2,
            card_mode: "none".into(),
            games_per_arm: 1000,
            anomalies: 0,
            replay_mismatches: 0,
            legal_action_violations: 0,
            observation_noninterference_passed: true,
            ci_reproducible: true,
            artifact_url: "https://example.invalid/run/1".into(),
        }],
    }
}

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
    assert!(manifest.seed_isolation_problems().is_empty());
    assert!(manifest.is_usable());
    // The manifest round-trips through JSON, because the gate reads it from disk.
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
    // Seed 120 is in the dataset but in no split: it must be an error, not a
    // default assignment to (say) test.
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
    // A different code revision is enough to make the set non-uniform: a learning
    // set that mixes engines cannot be compared with itself.
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
    // Independent of the manifest counters, re-derive samples and check them.
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
    // Simulate an edited manifest: the same opening appears in train and test.
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

#[test]
fn g0_is_hold_not_pass_when_evidence_is_missing() {
    let manifest = manifest();
    let report = evaluate_g0(&manifest, &inspect_dataset(&dataset()), None);
    assert_eq!(report.verdict, GateStatus::Hold);
    assert!(report.not_a_strength_claim);
    let (_, fails, holds) = report.counts();
    assert_eq!(fails, 0, "no data problem should be reported as a failure");
    assert!(holds > 0, "missing evidence must be held");
    let data_checks = report
        .checks
        .iter()
        .filter(|check| check.source != "evidence")
        .collect::<Vec<_>>();
    assert!(
        data_checks
            .iter()
            .all(|check| check.status == GateStatus::Pass),
        "the data side should be clean: {data_checks:?}"
    );
}

#[test]
fn g0_passes_only_with_complete_matching_evidence() {
    let manifest = manifest();
    let report = evaluate_g0(
        &manifest,
        &inspect_dataset(&dataset()),
        Some(&evidence("test-sha")),
    );
    assert_eq!(
        report.verdict,
        GateStatus::Pass,
        "checks: {:?}",
        report.checks
    );
    assert_eq!(report.evaluated_modes, vec!["cardless_2p".to_string()]);
    assert!(report.next_action.contains("G1"));
}

#[test]
fn g0_fails_on_a_short_or_mismatched_evaluation() {
    let manifest = manifest();
    let dataset_report = inspect_dataset(&dataset());

    let mut short = evidence("test-sha");
    short.modes[0].games_per_arm = 999;
    let report = evaluate_g0(&manifest, &dataset_report, Some(&short));
    assert_eq!(report.verdict, GateStatus::Fail);
    assert!(report
        .checks
        .iter()
        .any(|check| check.id == "evaluation_scale" && check.status == GateStatus::Fail));

    let mut mismatched = evidence("another-sha");
    mismatched.modes[0].artifact_url.clear();
    let report = evaluate_g0(&manifest, &dataset_report, Some(&mismatched));
    assert_eq!(report.verdict, GateStatus::Fail);
    assert!(report
        .checks
        .iter()
        .any(|check| check.id == "artifact_traceable" && check.status == GateStatus::Fail));
}

#[test]
fn g0_fails_when_rules_or_legality_evidence_is_absent() {
    let manifest = manifest();
    let dataset_report = inspect_dataset(&dataset());
    let mut missing_rule_evidence = evidence("test-sha");
    missing_rule_evidence.m8_1_rule_evidence = false;
    let report = evaluate_g0(&manifest, &dataset_report, Some(&missing_rule_evidence));
    assert_eq!(report.verdict, GateStatus::Fail);

    let mut illegal = evidence("test-sha");
    illegal.modes[0].legal_action_violations = 1;
    let report = evaluate_g0(&manifest, &dataset_report, Some(&illegal));
    assert_eq!(report.verdict, GateStatus::Fail);
    assert!(report
        .checks
        .iter()
        .any(|check| check.id == "evaluation_legality" && check.status == GateStatus::Fail));
}

#[test]
fn g0_holds_when_no_supported_card_mode_was_evaluated() {
    let manifest = manifest();
    let dataset_report = inspect_dataset(&dataset());
    let mut only_unsupported = evidence("test-sha");
    only_unsupported.modes[0].card_mode = "experimental_pool".into();
    let report = evaluate_g0(&manifest, &dataset_report, Some(&only_unsupported));
    assert_eq!(report.verdict, GateStatus::Hold);
    assert!(report.evaluated_modes.is_empty());
}

#[test]
fn g0_fails_a_dataset_with_a_corrupt_record() {
    let manifest = manifest();
    let mut records = dataset();
    records[0].final_state_hash = "0".repeat(64);
    let report = evaluate_g0(
        &manifest,
        &inspect_dataset(&records),
        Some(&evidence("test-sha")),
    );
    assert_eq!(report.verdict, GateStatus::Fail);
    assert!(report
        .checks
        .iter()
        .any(|check| check.id == "dataset_integrity" && check.status == GateStatus::Fail));
}

#[test]
fn a_four_player_dataset_is_supported() {
    let records = [200, 201, 210, 211, 220, 221]
        .into_iter()
        .map(|seed| record(4, seed))
        .collect::<Vec<_>>();
    let plan = SplitPlan {
        train: (200, 201),
        validation: (210, 211),
        test: (220, 221),
    };
    let manifest = build_manifest(&records, plan).expect("4p split");
    assert_eq!(
        manifest.split(Split::Train).unwrap().episodes,
        2,
        "each seed is one game regardless of player count"
    );
    let report = evaluate_g0(&manifest, &inspect_dataset(&records), None);
    assert_eq!(report.verdict, GateStatus::Hold);
}
