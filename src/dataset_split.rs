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
    /// Player counts present in the dataset. The G0 gate needs them to reject a
    /// mode whose condition does not match the data.
    #[serde(default)]
    pub player_counts: BTreeSet<usize>,
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

    /// Player counts the dataset actually contains.
    pub fn player_counts(&self) -> BTreeSet<usize> {
        self.player_counts.clone()
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
        player_counts: records.iter().map(|record| record.config.players).collect(),
        private_sample_leaks: leaks,
        issues,
        splits,
        // A paired evaluation row is one (seed block, arm, seat) game, so a block
        // legitimately holds `players * 2` games for one comparison. G0 checks
        // that shape instead of assuming one game per seed.
        seat_rotation_note: "One seed block holds one game per candidate seat for \
            each arm (players * 2 rows). Evaluations are validated per (block, arm, \
            seat), and splitting by seed never separates the seats of one block."
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

/// Version of the evidence file. v2 replaced free-form declarations with
/// references to the actual evaluation output (#167 C review P1).
pub const G0_EVIDENCE_SCHEMA_VERSION: u32 = 2;
/// Version of the attested checks file. Bumped when the checks layout changes.
pub const CHECKS_SCHEMA_VERSION: u32 = 2;

/// One row of a `paired_eval` matches file, reduced to the fields the gate
/// recomputes. Unknown fields are ignored, so the gate reads the artifact itself
/// rather than a transcription of it.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PairedRow {
    pub block_index: usize,
    pub seed: u64,
    #[serde(default)]
    pub arm_id: String,
    pub candidate_seat: usize,
    pub players: usize,
    #[serde(default)]
    pub cards: String,
    #[serde(default)]
    pub finished: bool,
    #[serde(default)]
    pub rounds_completed: u8,
    #[serde(default)]
    pub replay_verified: bool,
}

/// What was actually observed in one mode's matches files.
///
/// Every value here is recomputed from the rows, so it cannot be typed in by
/// hand. This is what replaces the old `games_per_arm: 1000` declaration.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ModeRecompute {
    pub rows: usize,
    pub candidate_rows: usize,
    pub baseline_rows: usize,
    pub blocks: BTreeSet<usize>,
    pub seeds: BTreeSet<u64>,
    /// Seat indexes that appeared anywhere.
    pub seats: BTreeSet<usize>,
    /// `arm_id` values that are neither `candidate` nor `baseline`.
    pub unknown_arms: BTreeSet<String>,
    pub players: BTreeSet<usize>,
    pub cards: BTreeSet<String>,
    /// `(block, arm, seat)` seen more than once.
    pub duplicate_keys: Vec<(usize, String, usize)>,
    pub unfinished: usize,
    pub unverified: usize,
    pub wrong_rounds: usize,
    /// Blocks that do not contain both arms for **every** seat exactly once.
    pub incomplete_blocks: Vec<usize>,
    /// `(block, arm)` pairs whose seat set is not exactly `0..players-1`.
    pub incomplete_seat_coverage: Vec<(usize, String, Vec<usize>)>,
    /// Blocks that have no row at all for a required arm (first few, for the
    /// report detail).
    pub blocks_missing_arm: Vec<(usize, String)>,
    /// How many `(block, arm)` pairs were entirely absent.
    pub blocks_missing_arm_count: usize,
    /// Seat indexes outside `0..players-1` (or contradicting the player count).
    pub out_of_range_seats: BTreeSet<usize>,
    /// Blocks whose rows do not all carry the same seed.
    pub blocks_with_mixed_seeds: Vec<usize>,
    /// Seeds that appear in more than one block.
    pub seeds_in_multiple_blocks: Vec<u64>,
    /// Rows whose `seed - block_index` differs from the first offset seen. The
    /// harness writes `seed = seed_start + block_index`, so one experiment has
    /// exactly one offset; anything else is a hand-edited or reused seed.
    pub seed_offset_conflicts: usize,
    /// The offsets that were observed (for the report detail).
    pub seed_offsets: BTreeSet<i64>,
}

impl ModeRecompute {
    pub fn games_per_arm(&self) -> usize {
        self.candidate_rows.min(self.baseline_rows)
    }

    /// The single player count of the run, if the rows agree on one.
    pub fn player_count(&self) -> Option<usize> {
        if self.players.len() == 1 {
            self.players.iter().copied().next()
        } else {
            None
        }
    }

    /// A seed block holds one game per seat per arm, so a complete block of a
    /// 2-player comparison is `2 arms * 2 seats = 4` rows and a 4-player one is
    /// `2 arms * 4 seats = 8` rows. This is the seat-rotation contract the
    /// manifest documents.
    pub fn expected_rows(&self) -> usize {
        let players = self.player_count().unwrap_or(0);
        self.blocks.len() * players * 2
    }

    /// Every block holds one game per seat per arm, seeds map one-to-one onto
    /// blocks, and no row is unfinished, unreplayed or contradicted.
    pub fn is_complete(&self) -> bool {
        self.duplicate_keys.is_empty()
            && self.unfinished == 0
            && self.unverified == 0
            && self.wrong_rounds == 0
            && self.incomplete_blocks.is_empty()
            && self.incomplete_seat_coverage.is_empty()
            && self.blocks_missing_arm_count == 0
            && self.unknown_arms.is_empty()
            && self.out_of_range_seats.is_empty()
            && self.blocks_with_mixed_seeds.is_empty()
            && self.seeds_in_multiple_blocks.is_empty()
            && self.seed_offset_conflicts == 0
            && self.player_count().is_some()
            && self.rows == self.expected_rows()
    }
}

/// Recompute the facts of one mode from its rows.
///
/// Seat rotation (review P1-a) and the seed/block mapping (review P1-b) are
/// derived here rather than trusted, because "1000 games per arm" is only
/// meaningful if every block really contains every seat of every arm and no seed
/// block is counted twice.
pub fn recompute_mode(rows: &[PairedRow]) -> ModeRecompute {
    let mut report = ModeRecompute {
        rows: rows.len(),
        ..ModeRecompute::default()
    };
    let mut seen: BTreeMap<(usize, String, usize), usize> = BTreeMap::new();
    let mut per_block: BTreeMap<usize, BTreeMap<(String, usize), usize>> = BTreeMap::new();
    let mut block_seed: BTreeMap<usize, u64> = BTreeMap::new();
    let mut seed_block: BTreeMap<u64, usize> = BTreeMap::new();
    let mut expected_offset: Option<i64> = None;
    for row in rows {
        match row.arm_id.as_str() {
            "candidate" => report.candidate_rows += 1,
            "baseline" => report.baseline_rows += 1,
            // Anything else is not a comparison row. It is recorded and fails
            // the run instead of being silently ignored (#170 re-review P1).
            other => {
                report.unknown_arms.insert(other.to_string());
            }
        }
        report.blocks.insert(row.block_index);
        report.seeds.insert(row.seed);
        report.seats.insert(row.candidate_seat);
        report.players.insert(row.players);
        report.cards.insert(row.cards.clone());
        if !row.finished {
            report.unfinished += 1;
        }
        if !row.replay_verified {
            report.unverified += 1;
        }
        if row.rounds_completed != 14 {
            report.wrong_rounds += 1;
        }
        let key = (row.block_index, row.arm_id.clone(), row.candidate_seat);
        if seen.insert(key, 1).is_some() {
            report
                .duplicate_keys
                .push((row.block_index, row.arm_id.clone(), row.candidate_seat));
        }
        *per_block
            .entry(row.block_index)
            .or_default()
            .entry((row.arm_id.clone(), row.candidate_seat))
            .or_default() += 1;

        // One block, one seed; one seed, one block.
        match block_seed.get(&row.block_index) {
            Some(seed) if *seed != row.seed => {
                if !report.blocks_with_mixed_seeds.contains(&row.block_index) {
                    report.blocks_with_mixed_seeds.push(row.block_index);
                }
            }
            Some(_) => {}
            None => {
                block_seed.insert(row.block_index, row.seed);
            }
        }
        match seed_block.get(&row.seed) {
            Some(block) if *block != row.block_index => {
                if !report.seeds_in_multiple_blocks.contains(&row.seed) {
                    report.seeds_in_multiple_blocks.push(row.seed);
                }
            }
            Some(_) => {}
            None => {
                seed_block.insert(row.seed, row.block_index);
            }
        }
        // The harness writes `seed = seed_start + block_index`, so all rows of
        // one experiment share a single offset.
        let offset = row.seed as i64 - row.block_index as i64;
        report.seed_offsets.insert(offset);
        match expected_offset {
            Some(expected) if offset != expected => report.seed_offset_conflicts += 1,
            Some(_) => {}
            None => expected_offset = Some(offset),
        }
    }

    // Seat rotation: every block must hold exactly
    // `{("candidate", seat), ("baseline", seat) | seat in 0..players}`.
    // Comparing the whole key set, not just row counts or the seats of whichever
    // arms appeared, is what makes a run where candidate and baseline live in
    // different blocks fail (#170 re-review P1).
    if let Some(players) = report.player_count() {
        let expected_seats: BTreeSet<usize> = (0..players).collect();
        let mut covered: BTreeMap<(usize, String), BTreeSet<usize>> = BTreeMap::new();
        let mut observed_keys: BTreeMap<usize, BTreeSet<(String, usize)>> = BTreeMap::new();
        for row in rows {
            if row.candidate_seat >= players {
                report.out_of_range_seats.insert(row.candidate_seat);
            }
            covered
                .entry((row.block_index, row.arm_id.clone()))
                .or_default()
                .insert(row.candidate_seat);
            observed_keys
                .entry(row.block_index)
                .or_default()
                .insert((row.arm_id.clone(), row.candidate_seat));
        }
        for ((block, arm), seats) in &covered {
            if seats != &expected_seats {
                report.incomplete_seat_coverage.push((
                    *block,
                    arm.clone(),
                    seats.iter().copied().collect(),
                ));
            }
        }
        let expected_keys: BTreeSet<(String, usize)> = ["candidate", "baseline"]
            .iter()
            .flat_map(|arm| {
                expected_seats
                    .iter()
                    .map(move |seat| ((*arm).to_string(), *seat))
            })
            .collect();
        for (block, keys) in &observed_keys {
            if keys != &expected_keys {
                report.incomplete_blocks.push(*block);
            }
            for arm in ["candidate", "baseline"] {
                if !keys.iter().any(|(observed, _)| observed == arm) {
                    report.blocks_missing_arm_count += 1;
                    if report.blocks_missing_arm.len() < 10 {
                        report.blocks_missing_arm.push((*block, arm.to_string()));
                    }
                }
            }
        }
        for block in report.blocks.iter() {
            // A block with no rows at all cannot be inspected above, but the
            // block set comes from the rows, so this only guards future edits.
            if !observed_keys.contains_key(block) {
                report.incomplete_blocks.push(*block);
            }
        }
        report.incomplete_blocks.sort_unstable();
        report.incomplete_blocks.dedup();
    } else {
        // Without a single player count nothing about seats can be verified.
        report.incomplete_blocks = report.blocks.iter().copied().collect();
    }
    report.blocks_with_mixed_seeds.sort_unstable();
    report.seeds_in_multiple_blocks.sort_unstable();
    report.incomplete_blocks.sort_unstable();
    report
}

/// Checks that cannot be derived from the matches rows. They are only accepted
/// when an independently produced checks file binds them to the same code
/// revision and the same matches bytes.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChecksEvidence {
    pub schema_version: u32,
    pub code_sha: String,
    pub matches_sha256: String,
    pub m8_rule_evidence: bool,
    pub m8_legal_api_evidence: bool,
    pub observation_noninterference_passed: bool,
    pub ci_reproducible: bool,
    /// How the four booleans above were derived. Without it the file is only a
    /// declaration, so the gate keeps those checks on `hold` instead of
    /// trusting a hand-written `true`.
    #[serde(default)]
    pub attestation: Option<ChecksAttestation>,
}

/// Version of the attestation block inside a checks file.
pub const ATTESTATION_SCHEMA_VERSION: u32 = 1;
/// The producer that is allowed to write an attestation. A checks file from any
/// other tool has no verified derivation, so it is not trusted.
pub const ATTESTATION_PRODUCER: &str = "scripts/m8_evidence.py";

/// Per-check test counts, all derived from one test-run report.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CheckAttestation {
    /// Issue the evidence answers, for traceability.
    pub issue: u32,
    pub required: usize,
    pub passed: usize,
    pub failed: usize,
    pub missing: usize,
}

/// Proof that the four attested booleans came from executed tests.
///
/// This is an attestation, not a formal proof: the gate cannot re-run a test
/// suite, so it verifies that the claim is *complete and internally
/// consistent* (every required test ran and passed, the report and requirement
/// digests are present, the run is a real Actions run) and that the file is
/// bound to the same code revision and matches bytes. A tampered boolean, a
/// missing test, a failed test or a re-bound file is rejected.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChecksAttestation {
    pub schema_version: u32,
    pub producer: String,
    /// GitHub Actions run that executed the tests.
    pub run_url: String,
    /// SHA-256 of the parsed test-run report the booleans were derived from.
    pub test_report_sha256: String,
    /// SHA-256 of the requirement manifest that defines the required tests.
    pub requirements_sha256: String,
    pub required_tests: usize,
    pub passed_tests: usize,
    pub failed_tests: usize,
    pub missing_tests: usize,
    /// Exact required test IDs by check, derived from the committed requirement
    /// manifest. Counts or an arbitrary digest are insufficient evidence.
    #[serde(default)]
    pub test_ids_by_check: BTreeMap<String, Vec<String>>,
    pub checks: BTreeMap<String, CheckAttestation>,
}

/// The four checks an attestation must cover, with the boolean field each one
/// maps to.
pub const ATTESTED_CHECKS: [&str; 4] = [
    "m8_rule_evidence",
    "m8_legal_api_evidence",
    "evaluation_observation_noninterference",
    "evaluation_ci_reproducible",
];

impl ChecksEvidence {
    /// The boolean this struct exposes for an attested check id.
    pub fn attested_value(&self, id: &str) -> Option<bool> {
        match id {
            "m8_rule_evidence" => Some(self.m8_rule_evidence),
            "m8_legal_api_evidence" => Some(self.m8_legal_api_evidence),
            "evaluation_observation_noninterference" => {
                Some(self.observation_noninterference_passed)
            }
            "evaluation_ci_reproducible" => Some(self.ci_reproducible),
            _ => None,
        }
    }
}

fn is_sha256_hex(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

/// Validate an attestation block against the checks file it belongs to.
///
/// Every failure is a contradiction (a claim that cannot be true as written),
/// so the caller reports `fail` rather than `hold`.
pub fn validate_attestation(
    attestation: &ChecksAttestation,
    checks: &ChecksEvidence,
) -> Result<(), String> {
    if attestation.schema_version != ATTESTATION_SCHEMA_VERSION {
        return Err(format!(
            "attestation schema {} is not {ATTESTATION_SCHEMA_VERSION}",
            attestation.schema_version
        ));
    }
    if attestation.producer != ATTESTATION_PRODUCER {
        return Err(format!("attestation producer {:?} is not {ATTESTATION_PRODUCER:?}",
            attestation.producer));
    }
    // A substring such as /actions/runs/ inside an unrelated URL is not a run.
    let prefix = "https://github.com/cueikusuta/agricola-re-ai/actions/runs/";
    let run_id = attestation.run_url.strip_prefix(prefix)
        .ok_or_else(|| "attestation run_url is not a repository Actions run".to_string())?;
    if run_id.is_empty() || run_id.starts_with('0')
        || !run_id.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err("attestation run_url has an invalid run id".into());
    }
    if !is_sha256_hex(&attestation.test_report_sha256) {
        return Err("attestation test_report_sha256 is not a SHA-256 hex digest".into());
    }
    if !is_sha256_hex(&attestation.requirements_sha256) {
        return Err("attestation requirements_sha256 is not a SHA-256 hex digest".into());
    }

    // Read the requirements compiled into this exact executable, not self-
    // reported counts from a checks JSON supplied by an arbitrary caller.
    let manifest_bytes = include_bytes!("../docs/m8-evidence-requirements.json");
    if attestation.requirements_sha256 != sha256_hex(manifest_bytes) {
        return Err("attestation requirements digest differs from committed manifest".into());
    }
    let requirements: serde_json::Value = serde_json::from_slice(manifest_bytes)
        .map_err(|error| format!("invalid compiled requirements: {error}"))?;
    if requirements["schema_version"].as_u64() != Some(1) {
        return Err("unsupported compiled requirement schema".into());
    }
    if attestation.checks.len() != ATTESTED_CHECKS.len()
        || attestation.test_ids_by_check.len() != ATTESTED_CHECKS.len() {
        return Err("attestation check or test-ID set is incomplete or contains extras".into());
    }
    let mut canonical = Vec::new();
    let mut required_total = 0usize;
    for id in ATTESTED_CHECKS {
        let manifest_check = &requirements["checks"][id];
        let expected_ids = manifest_check["tests"].as_array()
            .ok_or_else(|| format!("requirements lack tests for {id}"))?
            .iter().map(|test| test["id"].as_str()
                .map(str::to_owned)
                .ok_or_else(|| format!("requirements contain invalid test id in {id}")))
            .collect::<Result<Vec<_>, _>>()?;
        if expected_ids.is_empty() {
            return Err(format!("requirements list no tests for {id}"));
        }
        let provided_ids = attestation.test_ids_by_check.get(id)
            .ok_or_else(|| format!("attestation misses required test IDs for {id}"))?;
        if provided_ids != &expected_ids {
            return Err(format!("attestation test IDs differ from manifest for {id}"));
        }
        let entry = attestation.checks.get(id)
            .ok_or_else(|| format!("attestation has no entry for {id}"))?;
        let issue = manifest_check["issue"].as_u64()
            .ok_or_else(|| format!("requirements lack issue for {id}"))?;
        if entry.issue as u64 != issue {
            return Err(format!("attestation issue differs from requirements for {id}"));
        }
        if entry.required != expected_ids.len() || entry.passed != expected_ids.len()
            || entry.failed != 0 || entry.missing != 0 {
            return Err(format!("attestation {id} has inconsistent required/test counts"));
        }
        if checks.attested_value(id) != Some(true) {
            return Err(format!("attestation {id} contradicts a declared false check"));
        }
        required_total += expected_ids.len();
        for test_id in expected_ids {
            canonical.push(serde_json::json!({"id": test_id, "status": "passed"}));
        }
    }
    canonical.sort_by(|left, right| {
        left["id"].as_str().cmp(&right["id"].as_str())
    });
    let serialized = serde_json::to_vec(&canonical)
        .map_err(|error| format!("cannot serialize canonical tests: {error}"))?;
    if attestation.test_report_sha256 != sha256_hex(&serialized) {
        return Err("attestation test report digest is inconsistent with test IDs/outcomes".into());
    }
    if attestation.required_tests != required_total
        || attestation.passed_tests != required_total
        || attestation.failed_tests != 0
        || attestation.missing_tests != 0 {
        return Err(format!("attestation global totals do not match {} committed tests",
            required_total));
    }
    Ok(())
}

/// One evaluated condition. It points at real files; the gate reads them and
/// recomputes everything it can.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModeEvidence {
    pub mode: String,
    pub players: usize,
    pub card_mode: String,
    pub artifact_url: String,
    /// SHA-256 over the concatenated contents of `matches_paths`, computed by
    /// the producer. The gate recomputes it and refuses a mismatch.
    #[serde(default)]
    pub matches_sha256: String,
    /// Matches files, resolved relative to the evidence file's directory.
    #[serde(default)]
    pub matches_paths: Vec<String>,
    /// Optional attested checks file, bound by its hash and by the
    /// `code_sha`/`matches_sha256` it carries.
    #[serde(default)]
    pub checks_path: Option<String>,
    #[serde(default)]
    pub checks_sha256: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct G0Evidence {
    pub schema_version: u32,
    pub code_sha: String,
    pub modes: Vec<ModeEvidence>,
}

/// Evidence plus the bytes it points at, loaded by the caller.
#[derive(Clone, Debug, Default)]
pub struct ModeBundle {
    pub matches: Vec<Vec<u8>>,
    pub checks: Option<Vec<u8>>,
    /// Set when a declared file could not be read.
    pub load_error: Option<String>,
}

/// Normalise the `cards` spelling used by `paired_eval` rows to the dataset's
/// `card_mode` vocabulary.
pub fn normalise_card_mode(value: &str) -> &str {
    match value {
        "verified_basic_2p" | "verified" => "verified_basic",
        other => other,
    }
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}

/// Parse a matches file. An empty line is ignored; a malformed line is an error
/// so a truncated artifact cannot be mistaken for a smaller run.
pub fn load_matches(bytes: &[u8]) -> Result<Vec<PairedRow>, String> {
    let text = std::str::from_utf8(bytes).map_err(|error| error.to_string())?;
    let mut rows = Vec::new();
    for (index, line) in text.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let row: PairedRow = serde_json::from_str(line)
            .map_err(|error| format!("matches line {}: {error}", index + 1))?;
        rows.push(row);
    }
    if rows.is_empty() {
        return Err("matches file contains no rows".into());
    }
    Ok(rows)
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
    bundles: &BTreeMap<String, ModeBundle>,
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

    // --- Evaluation side. Every number below is recomputed from the matches
    // files the evidence points at; nothing is taken from a declaration. ---
    let Some(evidence) = evidence else {
        for id in [
            "evidence_schema",
            "evaluation_mode_conditions",
            "evaluation_matches_loaded",
            "evaluation_matches_digest",
            "evaluation_scale",
            "evaluation_completion",
            "evaluation_replay",
            "evaluation_block_completeness",
            "evaluation_seed_blocks",
            "evaluation_split_discipline",
            "checks_attested",
            "artifact_traceable",
        ] {
            checks.push(GateCheck::new(
                id,
                GateStatus::Hold,
                "evidence",
                "no evidence file was supplied; G0 cannot pass on data alone",
            ));
        }
        return finish(manifest, Vec::new(), checks, GateStatus::Hold);
    };

    if evidence.schema_version != G0_EVIDENCE_SCHEMA_VERSION {
        checks.push(GateCheck::new(
            "evidence_schema",
            GateStatus::Fail,
            "evidence",
            format!(
                "evidence schema {} is not {}; the declaration-only format cannot pass G0",
                evidence.schema_version, G0_EVIDENCE_SCHEMA_VERSION
            ),
        ));
        return finish(manifest, Vec::new(), checks, GateStatus::Fail);
    }
    checks.push(GateCheck::new(
        "evidence_schema",
        GateStatus::Pass,
        "evidence",
        format!("schema_version={G0_EVIDENCE_SCHEMA_VERSION}"),
    ));

    // Duplicate modes and mode/condition mismatches are errors, never a silent
    // "last one wins" or a quiet exclusion of a condition we cannot check.
    let mut seen_modes: BTreeSet<&str> = BTreeSet::new();
    let mut duplicates: Vec<&str> = Vec::new();
    for mode in &evidence.modes {
        if !seen_modes.insert(mode.mode.as_str()) {
            duplicates.push(mode.mode.as_str());
        }
    }
    if !duplicates.is_empty() {
        checks.push(GateCheck::new(
            "evaluation_mode_conditions",
            GateStatus::Fail,
            "evidence",
            format!("duplicate modes: {duplicates:?}"),
        ));
        return finish(manifest, Vec::new(), checks, GateStatus::Fail);
    }

    let manifest_conditions = manifest
        .provenance
        .card_modes
        .iter()
        .map(|mode| normalise_card_mode(mode).to_string())
        .collect::<BTreeSet<_>>();
    let dataset_players = manifest.player_counts();
    let mut mismatches = Vec::new();
    for mode in &evidence.modes {
        let normalised = normalise_card_mode(&mode.card_mode).to_string();
        if !SUPPORTED_CARD_MODES.contains(&normalised.as_str()) {
            mismatches.push(format!(
                "{}: unsupported card_mode {normalised:?}; unverified cards cannot \
                 enter an evaluation that gates learning",
                mode.mode
            ));
        } else if !manifest_conditions.contains(&normalised) {
            mismatches.push(format!(
                "{}: card_mode {normalised:?} does not match the dataset {:?}",
                mode.mode, manifest_conditions
            ));
        } else if !dataset_players.contains(&mode.players) {
            mismatches.push(format!(
                "{}: players {} does not match the dataset {:?}",
                mode.mode, mode.players, dataset_players
            ));
        }
    }
    if !mismatches.is_empty() {
        checks.push(GateCheck::new(
            "evaluation_mode_conditions",
            GateStatus::Fail,
            "evidence",
            mismatches.join("; "),
        ));
        return finish(manifest, Vec::new(), checks, GateStatus::Fail);
    }

    // "Implicit exclusion": if no supplied mode describes the dataset's own
    // condition, the gate stays on hold instead of passing on unrelated runs.
    let covers_dataset = evidence.modes.iter().any(|mode| {
        manifest_conditions.contains(normalise_card_mode(&mode.card_mode))
            && dataset_players.contains(&mode.players)
    });
    if !covers_dataset {
        checks.push(GateCheck::new(
            "evaluation_mode_conditions",
            GateStatus::Hold,
            "evidence",
            format!(
                "no evaluated mode covers the dataset condition (cards {:?}, players {:?})",
                manifest_conditions, dataset_players
            ),
        ));
        return finish(manifest, Vec::new(), checks, GateStatus::Hold);
    }
    checks.push(GateCheck::new(
        "evaluation_mode_conditions",
        GateStatus::Pass,
        "evidence",
        format!(
            "{} mode(s) match the dataset condition",
            evidence.modes.len()
        ),
    ));

    // The locked unit is the declared **test range**, not the handful of
    // episodes the learning dataset happens to contain: an evaluation draws many
    // unseen seeds from that range. Because the plan's ranges are validated as
    // disjoint, "inside test" also means "outside train and validation".
    let plan = manifest.plan.clone();
    let mut evaluated_modes = Vec::new();
    let mut recomputed: Vec<(String, ModeRecompute)> = Vec::new();

    for mode in &evidence.modes {
        let Some(bundle) = bundles.get(&mode.mode) else {
            checks.push(GateCheck::new(
                "evaluation_matches_loaded",
                GateStatus::Hold,
                "evidence",
                format!(
                    "{}: matches files were not supplied, so nothing can be recomputed",
                    mode.mode
                ),
            ));
            continue;
        };
        if let Some(error) = &bundle.load_error {
            checks.push(GateCheck::new(
                "evaluation_matches_loaded",
                GateStatus::Fail,
                "evidence",
                format!("{}: {error}", mode.mode),
            ));
            continue;
        }
        if mode.matches_paths.is_empty() || bundle.matches.is_empty() {
            checks.push(GateCheck::new(
                "evaluation_matches_loaded",
                GateStatus::Hold,
                "evidence",
                format!("{}: no matches file is declared", mode.mode),
            ));
            continue;
        }

        let digest = sha256_hex(&bundle.matches.concat());
        if mode.matches_sha256 != digest {
            checks.push(GateCheck::new(
                "evaluation_matches_digest",
                GateStatus::Fail,
                "evidence",
                format!(
                    "{}: declared sha256 {} does not match the supplied files ({})",
                    mode.mode, mode.matches_sha256, digest
                ),
            ));
            continue;
        }
        checks.push(GateCheck::new(
            "evaluation_matches_digest",
            GateStatus::Pass,
            "evidence",
            format!("{}: sha256 verified", mode.mode),
        ));

        let mut rows = Vec::new();
        let mut parse_error = None;
        for bytes in &bundle.matches {
            match load_matches(bytes) {
                Ok(mut parsed) => rows.append(&mut parsed),
                Err(error) => {
                    parse_error = Some(error);
                    break;
                }
            }
        }
        if let Some(error) = parse_error {
            checks.push(GateCheck::new(
                "evaluation_matches_loaded",
                GateStatus::Fail,
                "evidence",
                format!("{}: {error}", mode.mode),
            ));
            continue;
        }
        let facts = recompute_mode(&rows);
        evaluated_modes.push(mode.mode.clone());

        // Condition agreement between the declaration and the rows themselves.
        let row_players = facts.players.iter().copied().collect::<Vec<_>>();
        if row_players != vec![mode.players]
            || facts
                .cards
                .iter()
                .any(|cards| normalise_card_mode(cards) != normalise_card_mode(&mode.card_mode))
        {
            checks.push(GateCheck::new(
                "evaluation_mode_conditions",
                GateStatus::Fail,
                "evidence",
                format!(
                    "{}: declared players/cards {}/{} but the rows say {:?}/{:?}",
                    mode.mode, mode.players, mode.card_mode, row_players, facts.cards
                ),
            ));
            continue;
        }

        checks.push(GateCheck::new(
            "evaluation_scale",
            if facts.games_per_arm() >= G0_MIN_GAMES_PER_ARM {
                GateStatus::Pass
            } else {
                GateStatus::Fail
            },
            "evidence",
            format!(
                "{}: {} games/arm recomputed from {} rows ({} required); a smaller run \
                 is a plumbing test, not a G0 pass",
                mode.mode,
                facts.games_per_arm(),
                facts.rows,
                G0_MIN_GAMES_PER_ARM
            ),
        ));

        checks.push(GateCheck::new(
            "evaluation_completion",
            if facts.unfinished == 0 && facts.wrong_rounds == 0 {
                GateStatus::Pass
            } else {
                GateStatus::Fail
            },
            "evidence",
            format!(
                "{}: {} unfinished rows, {} rows without 14 rounds",
                mode.mode, facts.unfinished, facts.wrong_rounds
            ),
        ));

        checks.push(GateCheck::new(
            "evaluation_replay",
            if facts.unverified == 0 {
                GateStatus::Pass
            } else {
                GateStatus::Fail
            },
            "evidence",
            format!(
                "{}: {} rows without replay_verified",
                mode.mode, facts.unverified
            ),
        ));

        // Seat rotation (review P1-a): every block must contain **every** seat
        // of every arm, so a run that only ever used seat 0 cannot be counted as
        // a full rotation. The expected row count comes from the declared player
        // count, not from the seats that happened to appear.
        let players_label = facts
            .player_count()
            .map_or("?".to_string(), |count| count.to_string());
        checks.push(GateCheck::new(
            "evaluation_block_completeness",
            if facts.is_complete() {
                GateStatus::Pass
            } else {
                GateStatus::Fail
            },
            "evidence",
            format!(
                "{}: {} rows, expected {} for {} blocks x {} players x 2 arms; duplicate \
                 keys {:?}; incomplete blocks ({} total) {:?}; incomplete seat \
                 coverage {:?}; \
                 out-of-range seats {:?}; unknown arms {:?}; blocks missing an arm \
                 ({} total) {:?}",
                mode.mode,
                facts.rows,
                facts.expected_rows(),
                facts.blocks.len(),
                players_label,
                facts.duplicate_keys,
                // Only the head of a long list goes into the report detail; the
                // full list stays on the struct for tests and callers.
                facts.incomplete_blocks.len(),
                &facts.incomplete_blocks[..facts.incomplete_blocks.len().min(10)],
                facts.incomplete_seat_coverage,
                facts.out_of_range_seats,
                facts.unknown_arms,
                facts.blocks_missing_arm_count,
                facts.blocks_missing_arm
            ),
        ));

        // Seed/block integrity (review P1-b): one block one seed, one seed one
        // block, and one `seed - block_index` offset for the whole run. Without
        // this, rows that reuse a single seed would look like many independent
        // games.
        checks.push(GateCheck::new(
            "evaluation_seed_blocks",
            if facts.blocks_with_mixed_seeds.is_empty()
                && facts.seeds_in_multiple_blocks.is_empty()
                && facts.seed_offset_conflicts == 0
                && facts.seeds.len() == facts.blocks.len()
            {
                GateStatus::Pass
            } else {
                GateStatus::Fail
            },
            "evidence",
            format!(
                "{}: {} blocks, {} distinct seeds, offsets {:?}; blocks with mixed seeds \
                 {:?}; seeds in several blocks {:?}; rows with a different seed-block \
                 offset {}",
                mode.mode,
                facts.blocks.len(),
                facts.seeds.len(),
                facts.seed_offsets,
                facts.blocks_with_mixed_seeds,
                facts.seeds_in_multiple_blocks,
                facts.seed_offset_conflicts
            ),
        ));

        let leaked = facts
            .seeds
            .iter()
            .copied()
            .filter(|seed| plan.split_of(*seed) != Some(Split::Test))
            .collect::<Vec<_>>();
        checks.push(GateCheck::new(
            "evaluation_split_discipline",
            if leaked.is_empty() {
                GateStatus::Pass
            } else {
                GateStatus::Fail
            },
            "evidence",
            format!(
                "{}: {} of {} evaluated seeds are outside the locked test range                  [{}, {}]: {:?}",
                mode.mode,
                leaked.len(),
                facts.seeds.len(),
                plan.test.0,
                plan.test.1,
                &leaked[..leaked.len().min(8)]
            ),
        ));

        // Attested checks: only trusted when a checks file is bound to the same
        // code revision and the same matches bytes.
        let attestation = match (&bundle.checks, mode.checks_sha256.as_deref()) {
            (Some(bytes), Some(expected)) => {
                if sha256_hex(bytes) != expected {
                    Some(Err(
                        "declared checks sha256 does not match the file".to_string()
                    ))
                } else {
                    match serde_json::from_slice::<ChecksEvidence>(bytes) {
                        Ok(checks_file) => Some(Ok(checks_file)),
                        Err(_) => Some(Err("checks file is not valid evidence JSON".to_string())),
                    }
                }
            }
            _ => None,
        };
        match attestation {
            Some(Ok(checks_file)) => {
                // The checks file has its own schema (review P2). A file written
                // for an older layout carries fields we would not interpret, so
                // its booleans must not be trusted.
                if checks_file.schema_version != CHECKS_SCHEMA_VERSION {
                    checks.push(GateCheck::new(
                        "checks_attested",
                        GateStatus::Fail,
                        "evidence",
                        format!(
                            "{}: checks schema {} is not {}",
                            mode.mode, checks_file.schema_version, CHECKS_SCHEMA_VERSION
                        ),
                    ));
                } else if checks_file.code_sha != evidence.code_sha
                    || checks_file.matches_sha256 != digest
                {
                    checks.push(GateCheck::new(
                        "checks_attested",
                        GateStatus::Fail,
                        "evidence",
                        format!(
                            "{}: checks file is bound to code {:?}/matches {:?}, not {:?}/{:?}",
                            mode.mode,
                            checks_file.code_sha,
                            checks_file.matches_sha256,
                            evidence.code_sha,
                            digest
                        ),
                    ));
                } else {
                    // A checks file without an attestation is a declaration.
                    // Its booleans are not derived from executed tests, so they
                    // stay on hold exactly like an absent file (M8-1 evidence).
                    match checks_file.attestation.as_ref() {
                        None => {
                            for id in ATTESTED_CHECKS {
                                checks.push(GateCheck::new(
                                    id,
                                    GateStatus::Hold,
                                    "evidence",
                                    format!(
                                        "{}: no attestation; a hand-written boolean is not \
                                         evidence of an executed test run",
                                        mode.mode
                                    ),
                                ));
                            }
                        }
                        Some(attestation) => {
                            match validate_attestation(attestation, &checks_file) {
                                Err(error) => {
                                    checks.push(GateCheck::new(
                                        "checks_attested",
                                        GateStatus::Fail,
                                        "evidence",
                                        format!("{}: {error}", mode.mode),
                                    ));
                                    for id in ATTESTED_CHECKS {
                                        checks.push(GateCheck::new(
                                            id,
                                            GateStatus::Fail,
                                            "evidence",
                                            format!("{}: unattestable ({error})", mode.mode),
                                        ));
                                    }
                                }
                                Ok(()) => {
                                    for id in ATTESTED_CHECKS {
                                        let passed =
                                            checks_file.attested_value(id).unwrap_or(false);
                                        let entry = attestation.checks.get(id);
                                        checks.push(GateCheck::new(
                                            id,
                                            if passed {
                                                GateStatus::Pass
                                            } else {
                                                GateStatus::Fail
                                            },
                                            "evidence",
                                            format!(
                                                "{}: {} of {} required tests passed at code {:?} \
                                             ({}); run {}",
                                                mode.mode,
                                                entry.map(|entry| entry.passed).unwrap_or(0),
                                                entry.map(|entry| entry.required).unwrap_or(0),
                                                evidence.code_sha,
                                                checks_file.matches_sha256,
                                                attestation.run_url
                                            ),
                                        ));
                                    }
                                }
                            }
                        }
                    }
                }
            }
            Some(Err(error)) => checks.push(GateCheck::new(
                "checks_attested",
                GateStatus::Fail,
                "evidence",
                format!("{}: {error}", mode.mode),
            )),
            None => {
                for id in ATTESTED_CHECKS {
                    checks.push(GateCheck::new(
                        id,
                        GateStatus::Hold,
                        "evidence",
                        format!(
                            "{}: attested only (no checks file bound by hash); unverified \
                             claims cannot pass G0",
                            mode.mode
                        ),
                    ));
                }
            }
        }

        let traceable = !mode.artifact_url.is_empty()
            && !evidence.code_sha.is_empty()
            && manifest.provenance.code_shas.len() == 1
            && manifest.provenance.code_shas.contains(&evidence.code_sha);
        checks.push(GateCheck::new(
            "artifact_traceable",
            if traceable {
                GateStatus::Pass
            } else {
                GateStatus::Fail
            },
            "evidence",
            format!(
                "{}: artifact_url_present={} evidence_sha={:?} manifest_sha={:?}",
                mode.mode,
                !mode.artifact_url.is_empty(),
                evidence.code_sha,
                manifest.provenance.code_shas
            ),
        ));

        recomputed.push((mode.mode.clone(), facts));
    }

    // Every evaluated mode must have produced a complete recomputation before
    // the gate may pass; a missing bundle leaves a hold and caps the verdict.
    let (_, fails, holds) = summarize(&checks);
    let verdict = if fails > 0 {
        GateStatus::Fail
    } else if holds > 0 || evaluated_modes.is_empty() || recomputed.is_empty() {
        GateStatus::Hold
    } else {
        GateStatus::Pass
    };
    finish(manifest, evaluated_modes, checks, verdict)
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
