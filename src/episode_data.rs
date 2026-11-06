//! M12 episode/learning-data foundation (Issue #106, M12 data layer).
//!
//! This module uses the unified evaluation API to drive new episodes. It
//! records one game as a versioned, machine-readable [`EpisodeRecord`], writes
//! and reads JSONL, verifies each record by replaying its committed event log,
//! detects corrupt/duplicate data, and converts records into per-viewer
//! [`TrainingSample`]s that never contain another player's private information.
//!
//! ## Trusted data vs. training input
//!
//! An [`EpisodeRecord`] is coordinator-owned: it keeps the seed, the card set
//! and the full committed event log so a game can be replayed byte-for-byte.
//! [`EpisodeRecord::training_samples`] returns only samples where the viewer was
//! the acting player, so the sample carries that viewer's own observation and
//! nothing from another hand. [`TrainingSample`] has no seed or event-log
//! field, so a model cannot be handed the hidden deck or an opponent's hand by
//! accident. Draft history is attached per viewer from
//! [`DraftEpisode::views`], which are themselves per-seat observations.

use crate::api::Episode;
use crate::cards::CardSet;
use crate::draft::{DraftHistoryEvent, DraftMode, DraftObservation, DraftState};
use crate::model::{Config, Event, Game, Observation};
use crate::policy::{DetailPolicy, SpacePolicy};
use crate::score::{standings_with_cards, Standings};
use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

/// On-disk version of one episode record. Bump when the JSON shape changes.
pub const EPISODE_SCHEMA_VERSION: u32 = 1;
/// Upper bound on recorded decisions in one game (14 rounds, four players).
const MAX_RECORDED_TURNS: usize = 320;
/// Machine-readable format marker.
pub const EPISODE_FORMAT: &str = "agricola-episode";

/// Coordinator-supplied provenance for one episode.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct EpisodeMeta {
    /// Git or build identifier of the code that produced the episode.
    pub code_sha: String,
    /// Card mode label, for example `none` or `verified_basic`.
    pub card_mode: String,
    /// Policy identifier per seat, in seat order.
    pub ai_versions: Vec<String>,
}

impl EpisodeMeta {
    pub fn new(
        code_sha: impl Into<String>,
        card_mode: impl Into<String>,
        ai_versions: Vec<String>,
    ) -> Self {
        Self {
            code_sha: code_sha.into(),
            card_mode: card_mode.into(),
            ai_versions,
        }
    }
}

/// One decision plus the acting player's own observation before it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TurnRecord {
    pub index: usize,
    pub player: usize,
    pub round: u8,
    /// Public state plus this acting player's own hand/cards only.
    pub observation: Observation,
    /// The committed action payload (never hidden state).
    pub action: serde_json::Value,
    pub before: String,
    pub after: String,
    /// Terminal first-place credit for the acting player, filled at finalise.
    pub reward: f64,
}

/// A completed draft's per-seat legitimate observations.
///
/// `views[seat]` is exactly what seat `seat` legally observed: its own picks and
/// its own packet history. It never contains another seat's private hand.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DraftEpisode {
    pub mode: DraftMode,
    pub ruleset_id: String,
    pub deck_version: String,
    pub pool_kind: String,
    pub state_hash: String,
    pub views: Vec<DraftObservation>,
}

impl DraftEpisode {
    /// Capture one seat-specific view per player from a completed draft.
    pub fn from_state(state: &DraftState, pool_kind: impl Into<String>) -> Result<Self, String> {
        if !state.is_complete() {
            return Err("draft is not complete".into());
        }
        let token = state.current_token();
        let views = (0..state.player_count)
            .map(|seat| state.observe(seat, token))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Self {
            mode: state.mode,
            ruleset_id: state.ruleset.id.clone(),
            deck_version: state.deck_version.clone(),
            pool_kind: pool_kind.into(),
            state_hash: state.state_hash(),
            views,
        })
    }
}

/// One fully recorded game.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct EpisodeRecord {
    pub schema_version: u32,
    pub format: String,
    pub engine_version: String,
    pub rules_version: String,
    pub code_sha: String,
    pub card_mode: String,
    pub config: Config,
    #[serde(default)]
    pub card_set: CardSet,
    pub ai_versions: Vec<String>,
    #[serde(default)]
    pub draft: Option<DraftEpisode>,
    pub turns: Vec<TurnRecord>,
    pub initial_state_hash: String,
    pub final_state_hash: String,
    pub finished: bool,
    pub rounds: u8,
    pub standings: Standings,
    /// Trusted replay payload. Never part of a [`TrainingSample`].
    pub events: Vec<Event>,
}

impl EpisodeRecord {
    /// Rebuild the record from a game and its recorded turns, filling terminal
    /// rewards from the standings.
    pub fn from_game(
        game: &Game,
        meta: &EpisodeMeta,
        mut turns: Vec<TurnRecord>,
        draft: Option<DraftEpisode>,
    ) -> Result<Self, String> {
        if meta.ai_versions.len() != game.public.players.len() {
            return Err("ai_versions must list one policy per seat".into());
        }
        let standings = standings_with_cards(&game.public.players, &game.card_set);
        let credit = first_credit(&standings.rank);
        for turn in &mut turns {
            turn.reward = credit.get(turn.player).copied().unwrap_or(0.0);
        }
        Ok(Self {
            schema_version: EPISODE_SCHEMA_VERSION,
            format: EPISODE_FORMAT.into(),
            engine_version: game.log.engine_version.clone(),
            rules_version: game.log.rules_version.clone(),
            code_sha: meta.code_sha.clone(),
            card_mode: meta.card_mode.clone(),
            config: game.config.clone(),
            card_set: game.card_set.clone(),
            ai_versions: meta.ai_versions.clone(),
            draft,
            turns,
            initial_state_hash: game.log.initial_state_hash.clone(),
            final_state_hash: game.state_hash(),
            finished: game.public.finished,
            rounds: game.public.round,
            standings,
            events: game.log.events.clone(),
        })
    }

    /// Replay the committed event log and compare the terminal state, the
    /// standings, and every turn/event boundary. Any mismatch is corruption.
    ///
    /// This is the tamper-detection gate for a record, so it does **not** trust
    /// the recorded [`TurnRecord`] fields. It recomputes the initial state,
    /// checks the recorded `initial_state_hash`, and, for every event,
    /// recomputes the acting player's [`Observation`] and requires the recorded
    /// observation, round, action payload and zero-based index to match. After
    /// the replay it checks the terminal metadata (`finished`, `rounds`,
    /// `final_state_hash`, `standings`), rejects non-finite rewards, and
    /// requires every `turn.reward` to equal the first-place credit recomputed
    /// from the replayed standings. Swapping in another seat's observation,
    /// editing an action payload, an index, the initial hash or a learning
    /// reward is therefore detected even when the replay hashes alone agree.
    ///
    /// Draft-seeded records are not yet produced: `EpisodeRecord` has no
    /// initial-hand field, so verification always rebuilds the dealt state from
    /// `config`/`card_set`. When draft-to-game records are integrated, this must
    /// store and replay the explicit initial hands (as
    /// `Game::replay_with_setup` does) instead of deriving them from the seed.
    pub fn verify(&self) -> Result<(), String> {
        if self.schema_version != EPISODE_SCHEMA_VERSION {
            return Err(format!(
                "unsupported episode schema version {}",
                self.schema_version
            ));
        }
        if self.format != EPISODE_FORMAT {
            return Err(format!("unexpected episode format {}", self.format));
        }
        if self.ai_versions.len() != self.config.players {
            return Err("ai_versions length does not match player count".into());
        }
        if self.turns.len() != self.events.len() {
            return Err("turn/event count mismatch".into());
        }
        if self.turns.is_empty() {
            return Err("episode record has no turns".into());
        }
        // The first committed event's pre-state is the initial state.
        if self.events[0].before != self.initial_state_hash {
            return Err("recorded initial state hash does not match the first event".into());
        }
        let mut game = Game::with_cards(self.config.clone(), self.card_set.clone())?;
        if game.state_hash() != self.initial_state_hash {
            return Err("recomputed initial state does not match the recorded state".into());
        }
        for (expected_index, (turn, event)) in self.turns.iter().zip(&self.events).enumerate() {
            if turn.index != expected_index {
                return Err(format!(
                    "turn at position {expected_index} has index {}",
                    turn.index
                ));
            }
            if event.player != Some(turn.player) {
                return Err(format!(
                    "turn {} player does not match its event",
                    turn.index
                ));
            }
            if event.before != turn.before || event.after != turn.after {
                return Err(format!("turn {} hash does not match its event", turn.index));
            }
            if turn.action != event.payload {
                return Err(format!(
                    "turn {} action does not match its committed event payload",
                    turn.index
                ));
            }
            let observation = game.observation(turn.player)?;
            if observation != turn.observation {
                return Err(format!(
                    "turn {} observation does not match the replayed state",
                    turn.index
                ));
            }
            if turn.round != observation.public.round {
                return Err(format!(
                    "turn {} round does not match the replayed state",
                    turn.index
                ));
            }
            game.apply_recorded(event)?;
        }
        if game.state_hash() != self.final_state_hash {
            return Err("replay does not reproduce the recorded final state".into());
        }
        if game.public.finished != self.finished {
            return Err("recorded finished flag does not match the replay".into());
        }
        if game.public.round != self.rounds {
            return Err("recorded rounds does not match the replay".into());
        }
        let standings = standings_with_cards(&game.public.players, &self.card_set);
        if standings != self.standings {
            return Err("replay standings differ from the recorded standings".into());
        }
        // The learning label must equal the first-place credit of the replayed
        // result; `NaN`/infinite/any other value is a corrupted reward.
        let credit = first_credit(&standings.rank);
        for turn in &self.turns {
            let expected = credit.get(turn.player).copied().unwrap_or(0.0);
            if !turn.reward.is_finite() {
                return Err(format!("turn {} has a non-finite reward", turn.index));
            }
            if turn.reward != expected {
                return Err(format!(
                    "turn {} reward {} does not match the replay credit {expected}",
                    turn.index, turn.reward
                ));
            }
        }
        Ok(())
    }

    /// Per-viewer learning samples. Only turns where `viewer` acted are used,
    /// so only that viewer's own observation is exposed.
    ///
    /// The record is re-verified before conversion, so a tampered observation,
    /// action, index, terminal flag or reward can never become a training
    /// sample. `verify()` is the single gate and there is intentionally no
    /// unchecked path. For large datasets this replays once per call; the future
    /// optimisation is to cache a proof of verification (for example a
    /// `VerifiedEpisodeRecord` value or a batch-level check), never to skip the
    /// check.
    pub fn training_samples(&self, viewer: usize) -> Result<Vec<TrainingSample>, String> {
        self.verify()?;
        if viewer >= self.config.players {
            return Err("invalid training viewer".into());
        }
        let draft = self
            .draft
            .as_ref()
            .and_then(|draft| draft.views.get(viewer))
            .cloned();
        let mut samples = Vec::new();
        for turn in &self.turns {
            if turn.player != viewer {
                continue;
            }
            samples.push(TrainingSample {
                schema_version: EPISODE_SCHEMA_VERSION,
                viewer,
                round: turn.round,
                observation: turn.observation.clone(),
                action: turn.action.clone(),
                reward: turn.reward,
                draft: draft.clone(),
            });
        }
        Ok(samples)
    }

    /// Serialize the draft history a single viewer legitimately observed, as a
    /// convenience for learners that want it separately from the samples.
    pub fn draft_history(&self, viewer: usize) -> Option<&[DraftHistoryEvent]> {
        self.draft
            .as_ref()
            .and_then(|draft| draft.views.get(viewer))
            .map(|view| view.own_observation_history.as_slice())
    }
}

/// A single training example. It has no seed and no event log, so it cannot
/// leak another hand or the hidden deck order.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TrainingSample {
    pub schema_version: u32,
    pub viewer: usize,
    pub round: u8,
    pub observation: Observation,
    pub action: serde_json::Value,
    pub reward: f64,
    /// The viewer's own draft observation, if the game was drafted.
    #[serde(default)]
    pub draft: Option<DraftObservation>,
}

impl TrainingSample {
    /// Fail if the sample somehow serializes private engine state.
    pub fn assert_public(&self) -> Result<(), String> {
        let json = serde_json::to_string(self).map_err(|error| error.to_string())?;
        for secret in [
            "hidden_round_deck",
            "round_seed",
            "action_seed",
            "delayed_well",
            "\"rng\"",
            "\"events\"",
        ] {
            if json.contains(secret) {
                return Err(format!("training sample leaked {secret}"));
            }
        }
        if let Some(owned) = self.observation.hand_counts.get(self.viewer) {
            if self.observation.hand.len() > *owned {
                return Err("training sample carries more cards than the viewer owns".into());
            }
        }
        Ok(())
    }
}

/// Aggregate health of a record set.
///
/// This is the machine-readable contract behind the episode-dataset Actions
/// artifact. It records integrity (replay/duplicates) and the provenance a
/// downstream aggregation can cross-check: the engine/rules/card/format
/// versions, the producing code revision, the seed range, and how many turns
/// and events were observed. Mixed provenance values mean the records were not
/// generated under identical conditions and must not be pooled silently.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DatasetReport {
    pub total: usize,
    pub finished: usize,
    pub schema_versions: BTreeSet<u32>,
    pub player_counts: BTreeSet<usize>,
    /// Seeds that appear in more than one record.
    pub duplicate_seeds: Vec<u64>,
    /// Verification failures as `"<index>: <error>"`.
    pub corrupt: Vec<String>,
    #[serde(default)]
    pub seed_min: Option<u64>,
    #[serde(default)]
    pub seed_max: Option<u64>,
    #[serde(default)]
    pub distinct_seeds: usize,
    #[serde(default)]
    pub total_turns: usize,
    #[serde(default)]
    pub total_events: usize,
    #[serde(default)]
    pub formats: BTreeSet<String>,
    #[serde(default)]
    pub engine_versions: BTreeSet<String>,
    #[serde(default)]
    pub rules_versions: BTreeSet<String>,
    #[serde(default)]
    pub card_modes: BTreeSet<String>,
    #[serde(default)]
    pub code_shas: BTreeSet<String>,
}

impl DatasetReport {
    pub fn is_clean(&self) -> bool {
        self.corrupt.is_empty() && self.duplicate_seeds.is_empty()
    }

    /// Every record is finished. A dataset with partially played games cannot
    /// be used as a complete-game learning/evaluation set.
    pub fn is_complete(&self) -> bool {
        self.total > 0 && self.finished == self.total
    }

    /// All records share one format/engine/rules/card/code revision. When this
    /// is `false`, do not compare or merge the records as one condition.
    pub fn has_uniform_provenance(&self) -> bool {
        self.formats.len() <= 1
            && self.engine_versions.len() <= 1
            && self.rules_versions.len() <= 1
            && self.card_modes.len() <= 1
            && self.code_shas.len() <= 1
    }
}

/// Inspect records: count, versions, duplicate seeds and replay integrity.
pub fn inspect_dataset(records: &[EpisodeRecord]) -> DatasetReport {
    let mut report = DatasetReport::default();
    let mut seeds = BTreeMap::<u64, usize>::new();
    for (index, record) in records.iter().enumerate() {
        report.total += 1;
        if record.finished {
            report.finished += 1;
        }
        report.schema_versions.insert(record.schema_version);
        report.player_counts.insert(record.config.players);
        report.formats.insert(record.format.clone());
        report.engine_versions.insert(record.engine_version.clone());
        report.rules_versions.insert(record.rules_version.clone());
        report.card_modes.insert(record.card_mode.clone());
        report.code_shas.insert(record.code_sha.clone());
        report.total_turns += record.turns.len();
        report.total_events += record.events.len();
        let seed = record.config.seed;
        *seeds.entry(seed).or_default() += 1;
        report.seed_min = Some(report.seed_min.map_or(seed, |min| min.min(seed)));
        report.seed_max = Some(report.seed_max.map_or(seed, |max| max.max(seed)));
        if let Err(error) = record.verify() {
            report.corrupt.push(format!("{index}: {error}"));
        }
    }
    report.distinct_seeds = seeds.len();
    report.duplicate_seeds = seeds
        .into_iter()
        .filter(|(_, count)| *count > 1)
        .map(|(seed, _)| seed)
        .collect();
    report
}

/// Serialize records as one JSON object per line.
pub fn to_jsonl(records: &[EpisodeRecord]) -> Result<String, String> {
    let mut out = String::new();
    for record in records {
        out.push_str(&serde_json::to_string(record).map_err(|error| error.to_string())?);
        out.push('\n');
    }
    Ok(out)
}

/// Parse JSONL, reporting the offending line and rejecting mixed versions.
pub fn from_jsonl(text: &str) -> Result<Vec<EpisodeRecord>, String> {
    let mut records = Vec::new();
    let mut version: Option<u32> = None;
    for (index, line) in text.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let record: EpisodeRecord =
            serde_json::from_str(line).map_err(|error| format!("line {}: {error}", index + 1))?;
        match version {
            None => version = Some(record.schema_version),
            Some(expected) if expected != record.schema_version => {
                return Err(format!(
                    "line {}: schema version {} differs from {expected}",
                    index + 1,
                    record.schema_version
                ))
            }
            _ => {}
        }
        records.push(record);
    }
    if records.is_empty() {
        return Err("no episode records found".into());
    }
    Ok(records)
}

/// Play one full game with the supplied policies, recording each decision's
/// pre-action observation. `SimpleDetails` and `EvaluationPolicy` are typical
/// card-less choices; callers pass them explicitly so this module never
/// hard-codes a policy version.
pub fn record_self_play(
    config: Config,
    card_set: CardSet,
    meta: EpisodeMeta,
    space_policy: &dyn SpacePolicy,
    detail_policy: &dyn DetailPolicy,
) -> Result<EpisodeRecord, String> {
    record_episode(config, card_set, meta, space_policy, detail_policy)
}

/// Drive the M12 [`Episode`] API and record every decision into an
/// [`EpisodeRecord`].
///
/// This is the integration point between the evaluation API and the
/// learning-data layer: the coordinator owns the [`Episode`], policies see only
/// the acting player's [`Observation`] plus candidate actions, and the engine
/// validates and applies each action. The recorded turn observations therefore
/// contain only the acting player's own hand, never another player's cards, the
/// hidden round order or the internal RNG.
pub fn record_episode(
    config: Config,
    card_set: CardSet,
    meta: EpisodeMeta,
    space_policy: &dyn SpacePolicy,
    detail_policy: &dyn DetailPolicy,
) -> Result<EpisodeRecord, String> {
    let mut episode = Episode::reset_with_cards(config, card_set)?;
    record_from_episode(&mut episode, &meta, space_policy, detail_policy)
}

/// Record a fresh (no committed events) [`Episode`] through to its terminal
/// state. Mixed histories are rejected so the record always owns a complete
/// event log that [`EpisodeRecord::verify`] can replay.
pub fn record_from_episode(
    episode: &mut Episode,
    meta: &EpisodeMeta,
    space_policy: &dyn SpacePolicy,
    detail_policy: &dyn DetailPolicy,
) -> Result<EpisodeRecord, String> {
    if !episode.events().is_empty() {
        return Err("recording requires a fresh episode with no committed events".into());
    }
    let mut turns = Vec::new();
    for index in 0..MAX_RECORDED_TURNS {
        if episode.is_terminal() {
            break;
        }
        let player = episode.active_player();
        let input = episode.policy_input(player)?;
        let decision_index = episode.events().len() as u64;
        let mut choice_rng = ChaCha8Rng::seed_from_u64(
            episode.game().log.action_seed ^ decision_index.wrapping_mul(0x9E37_79B9_7F4A_7C15),
        );
        let space = space_policy
            .select(&input.observation, &input.legal_actions, &mut choice_rng)
            .ok_or("policy refused selection")?;
        let action = detail_policy
            .produce(&input.observation, space)
            .ok_or("policy refused details")?;
        episode.step(player, action)?;
        let event = episode
            .events()
            .last()
            .ok_or("applying an action produced no event")?
            .clone();
        turns.push(TurnRecord {
            index,
            player,
            round: input.observation.public.round,
            observation: input.observation,
            action: event.payload.clone(),
            before: event.before.clone(),
            after: event.after.clone(),
            reward: 0.0,
        });
    }
    if !episode.is_terminal() {
        return Err("recording did not reach the end of the game".into());
    }
    EpisodeRecord::from_game(episode.game(), meta, turns, None)
}

/// `1/k` first-place credit for each of the `k` tied leaders, else `0`.
pub fn first_credit(rank: &[usize]) -> Vec<f64> {
    let leaders = rank.iter().filter(|&&r| r == 1).count().max(1);
    rank.iter()
        .map(|&r| if r == 1 { 1.0 / leaders as f64 } else { 0.0 })
        .collect()
}
