//! M12/M14 train/validation/test split manifest and the M14 G0 gate (#167 C).
//!
//! `src/episode_data.rs` already defines one verified episode per game and the
//! observation-only `TrainingSample`. This module adds the two things the
//! roadmap's data-split section and the G0 gate ask for:
//!
//! 1. **A split manifest.** Seeds are assigned to `train` / `validation` /
//!    `test` by explicit inclusive ranges, everything else is rejected. The
//!    manifest proves the properties a learner needs: no seed in two splits, no
//!    initial state shared across splits, one provenance (engine/rules/card
//!    mode/deck/schema/policy) for the whole dataset, every episode replayed,
//!    and every training sample free of private information.
//! 2. **A machine-readable G0 verdict.** `M14 G0` is a gate, not a report card:
//!    a missing evidence file is `Hold`, a contradiction is `Fail`, and only a
//!    fully evidenced dataset is `Pass`. Nothing here trains a model or claims
//!    strength.
//!
//! The split is the *only* place a seed decides where a game goes, so a run
//! cannot accidentally train on its own test set by re-generating data "in the
//! same range": a seed outside the declared ranges is an error, not a default.
use crate::episode_data::{inspect_dataset, DatasetReport, EpisodeRecord};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

/// On-disk version of the split manifest shape.
pub const SPLIT_SCHEMA_VERSION: u32 = 1;
/// On-disk version of the G0 report shape.
pub const G0_SCHEMA_VERSION: u32 = 1;
/// A split this coarse is never a final evaluation; the roadmap's initial
/// evaluation is 1,000 games per arm.
pub const G0_MIN_GAMES_PER_ARM: usize = 1000;
/// The card conditions that may enter a learning dataset at all. A card pool
/// that has not passed official verification cannot be mixed in silently.
pub const SUPPORTED_CARD_MODES: [&str; 2] = ["none", "verified_basic"];

/// Which part of an experiment a game belongs to.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Split {
    #[default]
    Train,
    Validation,
    Test,
}

impl Split {
    pub fn as_str(self) -> &'static str {
        match self {
            Split::Train => "train",
            Split::Validation => "validation",
            Split::Test => "test",
        }
    }
}

/// Inclusive seed ranges, one per split.
///
/// Ranges must be disjoint; a seed in no range, or in two, is rejected instead
/// of being silently assigned.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SplitPlan {
    pub train: (u64, u64),
    pub validation: (u64, u64),
    pub test: (u64, u64),
}

impl SplitPlan {
    pub fn ranges(&self) -> [(Split, (u64, u64)); 3] {
        [
            (Split::Train, self.train),
            (Split::Validation, self.validation),
            (Split::Test, self.test),
        ]
    }

    /// The split a seed belongs to, or `None` when it belongs to none.
    pub fn split_of(&self, seed: u64) -> Option<Split> {
        self.ranges()
            .into_iter()
            .find(|(_, (low, high))| (*low..=*high).contains(&seed))
            .map(|(split, _)| split)
    }

    /// Ranges that share a seed. Only possible when they overlap.
    pub fn overlapping_ranges(&self) -> Vec<(Split, Split)> {
        let ranges = self.ranges();
        let mut overlaps = Vec::new();
        for (index, (left, left_range)) in ranges.iter().enumerate() {
            for (right, right_range) in ranges.iter().skip(index + 1) {
                if left_range.0 <= right_range.1 && right_range.0 <= left_range.1 {
                    overlaps.push((*left, *right));
                }
            }
        }
        overlaps
    }
}

/// Provenance a manifest pins so splits cannot be quietly pooled.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ManifestProvenance {
    pub formats: BTreeSet<String>,
    pub engine_versions: BTreeSet<String>,
    pub rules_versions: BTreeSet<String>,
    pub card_modes: BTreeSet<String>,
    pub card_set_hashes: BTreeSet<String>,
    pub schema_versions: BTreeSet<u32>,
    pub code_shas: BTreeSet<String>,
    /// Seat-ordered policy labels, joined with `>`, per distinct seat line-up.
    pub ai_versions: BTreeSet<String>,
}

impl ManifestProvenance {
    /// Every provenance field must have exactly one value for the whole
    /// dataset. Two engine versions or two decks in one learning set would make
    /// any later comparison uninterpretable.
    pub fn inconsistencies(&self) -> Vec<String> {
        let fields: [(&str, usize); 7] = [
            ("formats", self.formats.len()),
            ("engine_versions", self.engine_versions.len()),
            ("rules_versions", self.rules_versions.len()),
            ("card_modes", self.card_modes.len()),
            ("card_set_hashes", self.card_set_hashes.len()),
            ("schema_versions", self.schema_versions.len()),
            ("code_shas", self.code_shas.len()),
        ];
        let mut problems = Vec::new();
        for (name, count) in fields {
            if count > 1 {
                problems.push(format!("mixed {name}: {count} distinct values"));
            }
        }
        if self.ai_versions.len() > 1 {
            problems.push(format!(
                "mixed ai_versions: {} distinct seat line-ups",
                self.ai_versions.len()
            ));
        }
        problems
    }
}

/// One split's contents.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SplitContents {
    pub split: Split,
    pub seeds: Vec<u64>,
    pub episodes: usize,
    pub training_samples: usize,
    pub initial_state_hashes: BTreeSet<String>,
}

/// The complete, checked split manifest.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DatasetSplitManifest {
    pub schema_version: u32,
    pub format: String,
    pub plan: SplitPlan,
    pub provenance: ManifestProvenance,
    pub splits: Vec<SplitContents>,
    pub total_episodes: usize,
    pub total_training_samples: usize,
    /// Sampled `TrainingSample` values that failed the public-only check.
    pub private_sample_leaks: Vec<String>,
    /// Blocking inconsistencies. A manifest with any entry is not usable and the
    /// builder returns it as an error instead.
    pub issues: Vec<String>,
    pub seat_rotation_note: String,
}

impl DatasetSplitManifest {
    pub fn split(&self, split: Split) -> Option<&SplitContents> {
        self.splits.iter().find(|entry| entry.split == split)
    }

    pub fn is_usable(&self) -> bool {
        self.issues.is_empty() && self.private_sample_leaks.is_empty()
    }

    /// Seeds are disjoint by construction, but the manifest is a file on disk
    /// and may have been edited, so re-check it before trusting it.
    pub fn seed_isolation_problems(&self) -> Vec<String> {
        let mut seen: BTreeMap<u64, Split> = BTreeMap::new();
        let mut problems = Vec::new();
        for contents in &self.splits {
            for &seed in &contents.seeds {
                match seen.get(&seed) {
                    Some(other) if *other != contents.split => problems.push(format!(
                        "seed {seed} appears in {} and {}",
                        other.as_str(),
                        contents.split.as_str()
                    )),
                    Some(_) => problems.push(format!(
                        "seed {seed} is duplicated inside {}",
                        contents.split.as_str()
                    )),
                    None => {
                        seen.insert(seed, contents.split);
                    }
                }
            }
        }
        problems
    }

    /// The same initial state in two splits is leakage: the test split would
    /// contain a position the learner has already seen.
    pub fn initial_state_isolation_problems(&self) -> Vec<String> {
        let mut owners: BTreeMap<&str, Split> = BTreeMap::new();
        let mut problems = Vec::new();
        for contents in &self.splits {
            for hash in &contents.initial_state_hashes {
                match owners.get(hash.as_str()) {
                    Some(other) if *other != contents.split => problems.push(format!(
                        "initial state {} shared by {} and {}",
                        &hash[..hash.len().min(12)],
                        other.as_str(),
                        contents.split.as_str()
                    )),
                    _ => {
                        owners.insert(hash.as_str(), contents.split);
                    }
                }
            }
        }
        problems
    }
}

/// Assign every record to a split and prove the split is usable.
///
/// Fails closed (returns `Err`) on any contradiction: an unassigned seed, an
/// overlapping plan, mixed provenance, a duplicate seed, an unfinished or
/// non-replaying episode, a private leak in a training sample, or an empty
/// split. A caller therefore never receives a manifest it could mistake for a
/// validated one.
pub fn build_manifest(
    records: &[EpisodeRecord],
    plan: SplitPlan,
) -> Result<DatasetSplitManifest, String> {
    if records.is_empty() {
        return Err("cannot split an empty dataset".into());
    }
    let overlaps = plan.overlapping_ranges();
    if !overlaps.is_empty() {
        return Err(format!(
            "split ranges overlap: {overlaps:?} (a seed must belong to exactly one split)"
        ));
    }

    let mut provenance = ManifestProvenance::default();
    let mut contents: BTreeMap<Split, SplitContents> = BTreeMap::new();
    for split in [Split::Train, Split::Validation, Split::Test] {
        contents.insert(
            split,
            SplitContents {
                split,
                ..SplitContents::default()
            },
        );
    }
    let mut seeds_seen: BTreeMap<u64, usize> = BTreeMap::new();
    let mut leaks = Vec::new();

    for (index, record) in records.iter().enumerate() {
        record
            .verify()
            .map_err(|error| format!("record {index}: {error}"))?;
        let seed = record.config.seed;
        let split = plan
            .split_of(seed)
            .ok_or_else(|| format!("seed {seed} is outside every declared split range"))?;
        if seeds_seen.insert(seed, index).is_some() {
            return Err(format!("seed {seed} appears more than once in the dataset"));
        }

        provenance.formats.insert(record.format.clone());
        provenance
            .engine_versions
            .insert(record.engine_version.clone());
        provenance
            .rules_versions
            .insert(record.rules_version.clone());
        provenance.card_modes.insert(record.card_mode.clone());
        provenance
            .card_set_hashes
            .insert(record.card_set.content_hash());
        provenance.schema_versions.insert(record.schema_version);
        provenance.code_shas.insert(record.code_sha.clone());
        provenance.ai_versions.insert(record.ai_versions.join(">"));

        let entry = contents
            .get_mut(&split)
            .ok_or_else(|| format!("unknown split for seed {seed}"))?;
        entry.seeds.push(seed);
        entry.episodes += 1;
        entry
            .initial_state_hashes
            .insert(record.initial_state_hash.clone());

        // Observation-only check: every sample this game yields must be public.
        for viewer in 0..record.config.players {
            let samples = record.training_samples(viewer)?;
            for sample in &samples {
                if let Err(error) = sample.assert_public() {
                    leaks.push(format!("seed {seed} viewer {viewer}: {error}"));
                }
            }
            entry.training_samples += samples.len();
        }
    }

    let mut issues = Vec::new();
    issues.extend(provenance.inconsistencies());
    for contents in contents.values() {
        if contents.episodes == 0 {
            issues.push(format!(
                "split {} is empty; every split must contain games",
                contents.split.as_str()
            ));
        }
        if contents.training_samples == 0 {
            issues.push(format!(
                "split {} produced no training samples",
                contents.split.as_str()
            ));
        }
    }
    for mode in &provenance.card_modes {
        if !SUPPORTED_CARD_MODES.contains(&mode.as_str()) {
            issues.push(format!(
                "card mode {mode:?} is not a verified condition; unverified cards \
                 must not enter a learning or evaluation dataset"
            ));
        }
    }

    let mut splits: Vec<SplitContents> = contents.into_values().collect();
    for entry in &mut splits {
        entry.seeds.sort_unstable();
    }
    let manifest = DatasetSplitManifest {
        schema_version: SPLIT_SCHEMA_VERSION,
        format: "agricola-split-manifest".into(),
        plan,
        provenance,
        total_episodes: records.len(),
        total_training_samples: splits.iter().map(|entry| entry.training_samples).sum(),
        private_sample_leaks: leaks,
        issues,
        splits,
        seat_rotation_note: "Rows are per seat-rotated game; a seed block contains one \
            game per candidate seat, so split by seed never separates seats of the same \
            block."
            .into(),
    };

    let mut problems = manifest.issues.clone();
    problems.extend(manifest.seed_isolation_problems());
    problems.extend(manifest.initial_state_isolation_problems());
    problems.extend(manifest.private_sample_leaks.clone());
    if !problems.is_empty() {
        return Err(format!(
            "split manifest is not usable: {}",
            problems.join("; ")
        ));
    }
    Ok(manifest)
}

/// Machine-readable verdict for the M14 G0 gate.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum GateStatus {
    Pass,
    Fail,
    /// The evidence needed to decide is missing. Not a pass.
    Hold,
}

impl GateStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            GateStatus::Pass => "pass",
            GateStatus::Fail => "fail",
            GateStatus::Hold => "hold",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GateCheck {
    pub id: String,
    pub status: GateStatus,
    pub detail: String,
    /// Where the evidence came from (`manifest`, `dataset_report`, `evidence`).
    pub source: String,
}

impl GateCheck {
    fn new(id: &str, status: GateStatus, source: &str, detail: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            status,
            source: source.into(),
            detail: detail.into(),
        }
    }
}

/// Evidence a mode must supply before G0 can pass. Every field is an explicit
/// measurement; nothing is inferred from the presence of the data alone.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModeEvidence {
    pub mode: String,
    pub players: usize,
    pub card_mode: String,
    pub games_per_arm: usize,
    pub anomalies: usize,
    pub replay_mismatches: usize,
    pub legal_action_violations: usize,
    pub observation_noninterference_passed: bool,
    pub ci_reproducible: bool,
    pub artifact_url: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct G0Evidence {
    pub schema_version: u32,
    pub code_sha: String,
    pub m8_1_rule_evidence: bool,
    pub m8_2_legal_api_evidence: bool,
    pub modes: Vec<ModeEvidence>,
}

impl G0Evidence {
    /// A mode only counts when its card condition is a verified one.
    pub fn supported_modes(&self) -> Vec<&ModeEvidence> {
        self.modes
            .iter()
            .filter(|mode| SUPPORTED_CARD_MODES.contains(&mode.card_mode.as_str()))
            .collect()
    }
}

/// The G0 report artifact.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct G0Report {
    pub schema_version: u32,
    pub gate: String,
    pub verdict: GateStatus,
    pub checks: Vec<GateCheck>,
    pub dataset_episodes: usize,
    pub dataset_training_samples: usize,
    pub evaluated_modes: Vec<String>,
    pub not_a_strength_claim: bool,
    pub next_action: String,
}

impl G0Report {
    pub fn counts(&self) -> (usize, usize, usize) {
        let mut pass = 0;
        let mut fail = 0;
        let mut hold = 0;
        for check in &self.checks {
            match check.status {
                GateStatus::Pass => pass += 1,
                GateStatus::Fail => fail += 1,
                GateStatus::Hold => hold += 1,
            }
        }
        (pass, fail, hold)
    }
}

/// Evaluate G0 from the manifest, the dataset report and the mode evidence.
///
/// `evidence = None` is `Hold`, never `Pass`: the gate exists to stop training
/// when the grounds are unproven.
pub fn evaluate_g0(
    manifest: &DatasetSplitManifest,
    dataset: &DatasetReport,
    evidence: Option<&G0Evidence>,
) -> G0Report {
    let mut checks = Vec::new();

    // --- Dataset side, provable from the records themselves. ---
    let integrity_ok = dataset.is_clean() && dataset.is_complete();
    checks.push(GateCheck::new(
        "dataset_integrity",
        if integrity_ok {
            GateStatus::Pass
        } else {
            GateStatus::Fail
        },
        "dataset_report",
        format!(
            "total={} finished={} corrupt={} duplicate_seeds={}",
            dataset.total,
            dataset.finished,
            dataset.corrupt.len(),
            dataset.duplicate_seeds.len()
        ),
    ));

    let usable = manifest.is_usable();
    checks.push(GateCheck::new(
        "split_manifest_valid",
        if usable {
            GateStatus::Pass
        } else {
            GateStatus::Fail
        },
        "manifest",
        format!(
            "issues={} private_leaks={}",
            manifest.issues.len(),
            manifest.private_sample_leaks.len()
        ),
    ));

    let provenance_problems = manifest.provenance.inconsistencies();
    checks.push(GateCheck::new(
        "provenance_uniform",
        if provenance_problems.is_empty() {
            GateStatus::Pass
        } else {
            GateStatus::Fail
        },
        "manifest",
        if provenance_problems.is_empty() {
            format!(
                "engine={:?} rules={:?} cards={:?} code={:?}",
                manifest.provenance.engine_versions,
                manifest.provenance.rules_versions,
                manifest.provenance.card_modes,
                manifest.provenance.code_shas
            )
        } else {
            provenance_problems.join("; ")
        },
    ));

    let seed_problems = manifest.seed_isolation_problems();
    checks.push(GateCheck::new(
        "seed_isolation",
        if seed_problems.is_empty() {
            GateStatus::Pass
        } else {
            GateStatus::Fail
        },
        "manifest",
        if seed_problems.is_empty() {
            format!(
                "train={} validation={} test={} seeds, disjoint",
                manifest.split(Split::Train).map_or(0, |s| s.seeds.len()),
                manifest
                    .split(Split::Validation)
                    .map_or(0, |s| s.seeds.len()),
                manifest.split(Split::Test).map_or(0, |s| s.seeds.len())
            )
        } else {
            seed_problems.join("; ")
        },
    ));

    let state_problems = manifest.initial_state_isolation_problems();
    checks.push(GateCheck::new(
        "initial_state_isolation",
        if state_problems.is_empty() {
            GateStatus::Pass
        } else {
            GateStatus::Fail
        },
        "manifest",
        if state_problems.is_empty() {
            "no initial state is shared across splits".to_string()
        } else {
            state_problems.join("; ")
        },
    ));

    let private_ok = manifest.private_sample_leaks.is_empty();
    checks.push(GateCheck::new(
        "public_training_samples",
        if private_ok {
            GateStatus::Pass
        } else {
            GateStatus::Fail
        },
        "manifest",
        if private_ok {
            format!(
                "{} training samples passed the public-only check",
                manifest.total_training_samples
            )
        } else {
            manifest.private_sample_leaks.join("; ")
        },
    ));

    let supported_card_mode = manifest
        .provenance
        .card_modes
        .iter()
        .all(|mode| SUPPORTED_CARD_MODES.contains(&mode.as_str()));
    checks.push(GateCheck::new(
        "verified_card_condition",
        if supported_card_mode {
            GateStatus::Pass
        } else {
            GateStatus::Fail
        },
        "manifest",
        format!("card_modes={:?}", manifest.provenance.card_modes),
    ));

    // --- Evaluation side, only provable from an explicit evidence file. ---
    let Some(evidence) = evidence else {
        for id in [
            "m8_rule_evidence",
            "m8_legal_api_evidence",
            "evaluation_scale",
            "evaluation_replay",
            "evaluation_legality",
            "evaluation_observation_noninterference",
            "evaluation_ci_reproducible",
            "artifact_traceable",
        ] {
            checks.push(GateCheck::new(
                id,
                GateStatus::Hold,
                "evidence",
                "no evidence file was supplied; G0 cannot pass on data alone",
            ));
        }
        return finish(manifest, evidence_modes(&[]), checks, GateStatus::Hold);
    };

    checks.push(GateCheck::new(
        "m8_rule_evidence",
        if evidence.m8_1_rule_evidence {
            GateStatus::Pass
        } else {
            GateStatus::Fail
        },
        "evidence",
        format!("m8_1_rule_evidence={}", evidence.m8_1_rule_evidence),
    ));
    checks.push(GateCheck::new(
        "m8_legal_api_evidence",
        if evidence.m8_2_legal_api_evidence {
            GateStatus::Pass
        } else {
            GateStatus::Fail
        },
        "evidence",
        format!(
            "m8_2_legal_api_evidence={}",
            evidence.m8_2_legal_api_evidence
        ),
    ));

    let modes = evidence.supported_modes();
    if modes.is_empty() {
        checks.push(GateCheck::new(
            "evaluation_scale",
            GateStatus::Hold,
            "evidence",
            "no supported (verified-card) mode was evaluated",
        ));
        checks.push(GateCheck::new(
            "evaluation_replay",
            GateStatus::Hold,
            "evidence",
            "no supported mode was evaluated",
        ));
        checks.push(GateCheck::new(
            "evaluation_legality",
            GateStatus::Hold,
            "evidence",
            "no supported mode was evaluated",
        ));
        checks.push(GateCheck::new(
            "evaluation_observation_noninterference",
            GateStatus::Hold,
            "evidence",
            "no supported mode was evaluated",
        ));
        checks.push(GateCheck::new(
            "evaluation_ci_reproducible",
            GateStatus::Hold,
            "evidence",
            "no supported mode was evaluated",
        ));
        checks.push(GateCheck::new(
            "artifact_traceable",
            GateStatus::Hold,
            "evidence",
            "no supported mode was evaluated",
        ));
        return finish(manifest, evidence_modes(&modes), checks, GateStatus::Hold);
    }

    let worst = |values: Vec<(String, bool)>| -> GateStatus {
        if values.iter().all(|(_, ok)| *ok) {
            GateStatus::Pass
        } else {
            GateStatus::Fail
        }
    };

    checks.push(GateCheck::new(
        "evaluation_scale",
        worst(
            modes
                .iter()
                .map(|mode| {
                    (
                        mode.mode.clone(),
                        mode.games_per_arm >= G0_MIN_GAMES_PER_ARM,
                    )
                })
                .collect(),
        ),
        "evidence",
        modes
            .iter()
            .map(|mode| format!("{}={} games/arm", mode.mode, mode.games_per_arm))
            .collect::<Vec<_>>()
            .join(", "),
    ));
    checks.push(GateCheck::new(
        "evaluation_replay",
        worst(
            modes
                .iter()
                .map(|mode| (mode.mode.clone(), mode.replay_mismatches == 0))
                .collect(),
        ),
        "evidence",
        modes
            .iter()
            .map(|mode| format!("{}={} mismatches", mode.mode, mode.replay_mismatches))
            .collect::<Vec<_>>()
            .join(", "),
    ));
    checks.push(GateCheck::new(
        "evaluation_legality",
        worst(
            modes
                .iter()
                .map(|mode| (mode.mode.clone(), mode.legal_action_violations == 0))
                .collect(),
        ),
        "evidence",
        modes
            .iter()
            .map(|mode| format!("{}={} violations", mode.mode, mode.legal_action_violations))
            .collect::<Vec<_>>()
            .join(", "),
    ));
    checks.push(GateCheck::new(
        "evaluation_observation_noninterference",
        worst(
            modes
                .iter()
                .map(|mode| (mode.mode.clone(), mode.observation_noninterference_passed))
                .collect(),
        ),
        "evidence",
        modes
            .iter()
            .map(|mode| format!("{}={}", mode.mode, mode.observation_noninterference_passed))
            .collect::<Vec<_>>()
            .join(", "),
    ));
    checks.push(GateCheck::new(
        "evaluation_ci_reproducible",
        worst(
            modes
                .iter()
                .map(|mode| (mode.mode.clone(), mode.ci_reproducible))
                .collect(),
        ),
        "evidence",
        modes
            .iter()
            .map(|mode| format!("{}={}", mode.mode, mode.ci_reproducible))
            .collect::<Vec<_>>()
            .join(", "),
    ));

    let anomalies_ok = modes.iter().all(|mode| mode.anomalies == 0);
    if !anomalies_ok {
        checks.push(GateCheck::new(
            "evaluation_anomalies",
            GateStatus::Fail,
            "evidence",
            modes
                .iter()
                .map(|mode| format!("{}={} anomalies", mode.mode, mode.anomalies))
                .collect::<Vec<_>>()
                .join(", "),
        ));
    }

    let traceable = !evidence.code_sha.is_empty()
        && manifest.provenance.code_shas.len() == 1
        && manifest.provenance.code_shas.contains(&evidence.code_sha)
        && modes.iter().all(|mode| !mode.artifact_url.is_empty());
    checks.push(GateCheck::new(
        "artifact_traceable",
        if traceable {
            GateStatus::Pass
        } else {
            GateStatus::Fail
        },
        "evidence",
        format!(
            "evidence_sha={:?} manifest_sha={:?} artifact_urls_present={}",
            evidence.code_sha,
            manifest.provenance.code_shas,
            modes.iter().all(|mode| !mode.artifact_url.is_empty())
        ),
    ));

    let (_, fails, holds) = summarize(&checks);
    let verdict = if fails > 0 {
        GateStatus::Fail
    } else if holds > 0 {
        GateStatus::Hold
    } else {
        GateStatus::Pass
    };
    finish(manifest, evidence_modes(&modes), checks, verdict)
}

fn evidence_modes(modes: &[&ModeEvidence]) -> Vec<String> {
    modes.iter().map(|mode| mode.mode.clone()).collect()
}

fn summarize(checks: &[GateCheck]) -> (usize, usize, usize) {
    let mut pass = 0;
    let mut fail = 0;
    let mut hold = 0;
    for check in checks {
        match check.status {
            GateStatus::Pass => pass += 1,
            GateStatus::Fail => fail += 1,
            GateStatus::Hold => hold += 1,
        }
    }
    (pass, fail, hold)
}

fn finish(
    manifest: &DatasetSplitManifest,
    evaluated_modes: Vec<String>,
    checks: Vec<GateCheck>,
    verdict: GateStatus,
) -> G0Report {
    let next_action = match verdict {
        GateStatus::Pass => {
            "proceed to G1 (teacher-signal value) on the train split only".to_string()
        }
        GateStatus::Fail => "fix the failing checks and re-run this gate".to_string(),
        GateStatus::Hold => {
            "produce the missing evidence (see the hold checks) and re-run this gate".to_string()
        }
    };
    G0Report {
        schema_version: G0_SCHEMA_VERSION,
        gate: "G0".into(),
        verdict,
        checks,
        dataset_episodes: manifest.total_episodes,
        dataset_training_samples: manifest.total_training_samples,
        evaluated_modes,
        not_a_strength_claim: true,
        next_action,
    }
}

/// Helper for callers that have records but not a report yet.
pub fn dataset_report(records: &[EpisodeRecord]) -> DatasetReport {
    inspect_dataset(records)
}

/// The provenance a caller can reuse when it only has a manifest-shaped input.
pub fn provenance_from_records(records: &[EpisodeRecord]) -> ManifestProvenance {
    let mut provenance = ManifestProvenance::default();
    for record in records {
        provenance.formats.insert(record.format.clone());
        provenance
            .engine_versions
            .insert(record.engine_version.clone());
        provenance
            .rules_versions
            .insert(record.rules_version.clone());
        provenance.card_modes.insert(record.card_mode.clone());
        provenance
            .card_set_hashes
            .insert(record.card_set.content_hash());
        provenance.schema_versions.insert(record.schema_version);
        provenance.code_shas.insert(record.code_sha.clone());
        provenance.ai_versions.insert(record.ai_versions.join(">"));
    }
    provenance
}
