//! M11 draft foundation: normal 7-card deal, 7-7 draft, 10-7 simultaneous
//! draft, and 10-7 non-draft choice.
//!
//! The authoritative [`DraftState`] owns the private packets, the deck seed,
//! the private discard pile, and every viewer's observation history. Only
//! [`DraftObservation`] may be shown to an AI or a remote viewer.
//!
//! This module is deliberately self-contained: it never reads card effects,
//! card text, or any hidden game state. It only carries opaque card IDs from a
//! caller-supplied [`DraftPool`]. Card implementation and eligibility live in
//! the card subsystem (#115/#139); this module must not import them.
//!
//! ## Unconfirmed rules stay configurable
//!
//! The 2016 Lookout rulebook documents the normal 7 + 7 deal only. The pass
//! direction, phase order, and simultaneous-vs-sequential timing of draft
//! variants are tournament conventions, not confirmed official rules
//! (`docs/draft-protocol-m11.md`). They are therefore *required* parameters on
//! [`DraftRuleset`] with no silent default. Callers that have not decided a
//! convention must not start a draft.

use rand::{seq::SliceRandom, Rng, SeedableRng};
use rand_chacha::ChaCha8Rng;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};

/// Version of the on-disk/server draft schema.
pub const DRAFT_SCHEMA_VERSION: u32 = 2;

/// A card is either an occupation or a minor improvement for draft purposes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CardKind {
    Occupation,
    MinorImprovement,
}

impl CardKind {
    pub fn as_str(self) -> &'static str {
        match self {
            CardKind::Occupation => "occupation",
            CardKind::MinorImprovement => "minor_improvement",
        }
    }
}

/// The four modes required by Issue #104.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DraftMode {
    /// Normal 7 occupations + 7 minor improvements, no selection.
    Deal7,
    /// 7-7 draft: pick 1 per kind per round, pass the rest to the neighbour.
    Draft77,
    /// 10-7 simultaneous: pick occupation + minor together, pass both packets
    /// only after every seat has committed.
    Draft10_7Simultaneous,
    /// 10-7 non-draft: pick 7 of your own 10 per kind with no passing.
    Choose10_7,
}

impl DraftMode {
    pub fn id(self) -> &'static str {
        match self {
            DraftMode::Deal7 => "deal_7",
            DraftMode::Draft77 => "draft_7_7",
            DraftMode::Draft10_7Simultaneous => "draft_10_7_simultaneous",
            DraftMode::Choose10_7 => "choose_10_7",
        }
    }

    /// Cards dealt to each player per kind.
    pub fn dealt_per_kind(self) -> usize {
        match self {
            DraftMode::Deal7 | DraftMode::Draft77 => 7,
            DraftMode::Draft10_7Simultaneous | DraftMode::Choose10_7 => 10,
        }
    }

    /// Number of picks each player makes per kind in the draft phase.
    pub fn picks_per_kind(self) -> usize {
        match self {
            DraftMode::Deal7 => 0,
            _ => 7,
        }
    }

    /// Cards kept in the final hand per kind (always 7).
    pub fn final_hand_per_kind(self) -> usize {
        7
    }
}

/// Direction a packet travels between seats.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Direction {
    Left,
    Right,
}

impl Direction {
    pub fn step(self, seat: usize, players: usize) -> usize {
        match self {
            // Seats are numbered in clockwise order (1P -> 2P -> 3P -> 4P).
            Direction::Left => (seat + players - 1) % players,
            Direction::Right => (seat + 1) % players,
        }
    }
}

/// Explicit house-rule parameters. There is no `Default` on purpose: the
/// tournament convention is still an open decision, so a caller must state it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DraftRuleset {
    /// Free-form identifier shown to players, e.g. `custom-explicit`.
    pub id: String,
    pub direction_occupation: Direction,
    pub direction_minor: Direction,
    /// For 7-7 only: occupations are drafted before minor improvements.
    pub occupation_phase_first: bool,
}

impl DraftRuleset {
    pub fn explicit(
        id: impl Into<String>,
        direction_occupation: Direction,
        direction_minor: Direction,
        occupation_phase_first: bool,
    ) -> Self {
        Self {
            id: id.into(),
            direction_occupation,
            direction_minor,
            occupation_phase_first,
        }
    }

    pub fn direction(&self, kind: CardKind) -> Direction {
        match kind {
            CardKind::Occupation => self.direction_occupation,
            CardKind::MinorImprovement => self.direction_minor,
        }
    }
}

/// Opaque, kind-separated candidate pools supplied by the caller.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DraftPool {
    pub occupations: Vec<String>,
    pub minors: Vec<String>,
}

impl DraftPool {
    pub fn new(occupations: Vec<String>, minors: Vec<String>) -> Self {
        Self {
            occupations,
            minors,
        }
    }

    pub fn kind_ids(&self, kind: CardKind) -> &[String] {
        match kind {
            CardKind::Occupation => &self.occupations,
            CardKind::MinorImprovement => &self.minors,
        }
    }

    /// Structural checks only. Individual card legality/provenance is the card
    /// subsystem's job (#115); this validator never marks a card "verified".
    pub fn validate(&self) -> Result<(), String> {
        if self.occupations.is_empty() || self.minors.is_empty() {
            return Err("draft pool needs occupations and minors".into());
        }
        let mut seen = BTreeSet::new();
        for id in self.occupations.iter().chain(self.minors.iter()) {
            if id.is_empty() {
                return Err("draft card id must not be empty".into());
            }
            if !seen.insert(id.clone()) {
                return Err(format!("duplicate draft card id: {id}"));
            }
        }
        Ok(())
    }

    pub fn check_supply(&self, mode: DraftMode, players: usize) -> Result<(), String> {
        self.validate()?;
        if !(2..=4).contains(&players) {
            return Err("draft players must be 2, 3 or 4".into());
        }
        let required = mode.dealt_per_kind() * players;
        for kind in [CardKind::Occupation, CardKind::MinorImprovement] {
            let available = self.kind_ids(kind).len();
            if available < required {
                return Err(format!(
                    "{} pool has {available} cards but {} needs {required}",
                    kind.as_str(),
                    mode.id()
                ));
            }
        }
        Ok(())
    }
}

/// Immutable parameters needed to rebuild a draft deterministically.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DraftSetup {
    pub mode: DraftMode,
    pub ruleset: DraftRuleset,
    pub pool: DraftPool,
    pub player_count: usize,
    pub seed: u64,
    pub deck_version: String,
}

impl DraftSetup {
    pub fn validate(&self) -> Result<(), String> {
        self.pool.check_supply(self.mode, self.player_count)?;
        if matches!(
            self.mode,
            DraftMode::Draft77 | DraftMode::Draft10_7Simultaneous
        ) && (self.ruleset.direction_occupation != Direction::Right
            || self.ruleset.direction_minor != Direction::Right)
        {
            return Err("draft packets must pass clockwise (right) for both kinds".into());
        }
        if self.ruleset.id.trim().is_empty() {
            return Err("draft ruleset id must not be empty".into());
        }
        if self.deck_version.trim().is_empty() {
            return Err("draft deck_version must not be empty".into());
        }
        Ok(())
    }

    /// Build the initial state: deal, record what each seat has seen, and set
    /// the first phase. Deterministic for a fixed seed and pool order.
    pub fn start(&self) -> Result<DraftState, String> {
        self.validate()?;
        let players = self.player_count;
        let mode = self.mode;
        let dealt = mode.dealt_per_kind();
        // Separate the pre-draft starting-seat draw from both card shuffles.
        // A fixed seed reproduces the choice without leaking the deck RNG.
        let mut starter_rng = ChaCha8Rng::seed_from_u64(self.seed ^ 0x5354_4152_545F_5031);
        let start_player = starter_rng.gen_range(0..players);
        let occupations = shuffle(&self.pool.occupations, self.seed, 0x0CC0_0001);
        let minors = shuffle(&self.pool.minors, self.seed, 0x0CC0_0002);
        let mut kind_of = BTreeMap::new();
        for id in &occupations {
            kind_of.insert(id.clone(), CardKind::Occupation);
        }
        for id in &minors {
            kind_of.insert(id.clone(), CardKind::MinorImprovement);
        }
        let mut packets = Vec::new();
        let mut packet_id = 0u32;
        for kind in [CardKind::Occupation, CardKind::MinorImprovement] {
            let deck = match kind {
                CardKind::Occupation => &occupations,
                CardKind::MinorImprovement => &minors,
            };
            for seat in 0..players {
                packets.push(DraftPacket {
                    packet_id,
                    kind,
                    owner_seat: seat,
                    cards: deck[seat * dealt..(seat + 1) * dealt].to_vec(),
                });
                packet_id += 1;
            }
        }
        let mut picked = vec![Vec::new(); players];
        let mut history: Vec<Vec<DraftHistoryEvent>> = vec![Vec::new(); players];
        if mode == DraftMode::Deal7 {
            for packet in &packets {
                picked[packet.owner_seat].extend(packet.cards.iter().cloned());
            }
            for packet in &mut packets {
                packet.cards.clear();
            }
        } else {
            // Every player has been dealt, and therefore legitimately seen,
            // their own starting packet for both kinds.
            for packet in &packets {
                history[packet.owner_seat].push(DraftHistoryEvent::SeenPacket {
                    round: 1,
                    kind: packet.kind,
                    ids: packet.cards.clone(),
                    packet_id: packet.packet_id,
                });
            }
        }
        let complete = mode == DraftMode::Deal7;
        let phase = match mode {
            DraftMode::Deal7 => DraftPhase::Complete,
            DraftMode::Choose10_7 => DraftPhase::Choosing,
            _ => DraftPhase::Drafting,
        };
        let active_kind = match mode {
            DraftMode::Draft77 => Some(if self.ruleset.occupation_phase_first {
                CardKind::Occupation
            } else {
                CardKind::MinorImprovement
            }),
            _ => None,
        };
        let state = DraftState {
            schema_version: DRAFT_SCHEMA_VERSION,
            mode,
            ruleset: self.ruleset.clone(),
            player_count: players,
            start_player,
            seed: self.seed,
            deck_version: self.deck_version.clone(),
            round: if complete { 0 } else { 1 },
            phase,
            active_kind,
            packets,
            picked,
            discarded: Vec::new(),
            commitments: vec![None; players],
            history,
            drafted_kinds: Vec::new(),
            kind_of,
            actions: Vec::new(),
            event_index: 0,
        };
        state.validate_invariants()?;
        Ok(state)
    }
}

/// A hand of cards currently owned (and possibly passed) by a seat.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DraftPacket {
    pub packet_id: u32,
    pub kind: CardKind,
    pub owner_seat: usize,
    pub cards: Vec<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DraftPhase {
    Drafting,
    Choosing,
    Complete,
}

/// A single seat's committed selection for the current round.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DraftCommit {
    pub occupation: Option<String>,
    pub minor: Option<String>,
}

/// Authoritative record of one accepted change, kept for replay.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum DraftAction {
    Pick {
        seat: usize,
        occupation: Option<String>,
        minor: Option<String>,
    },
    Choice {
        seat: usize,
        occupations: Vec<String>,
        minors: Vec<String>,
    },
}

/// A privacy-preserving, per-viewer history entry.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum DraftHistoryEvent {
    SeenPacket {
        round: u8,
        kind: CardKind,
        ids: Vec<String>,
        packet_id: u32,
    },
    OwnPick {
        round: u8,
        kind: CardKind,
        id: String,
    },
    PassedPacket {
        round: u8,
        kind: CardKind,
        ids: Vec<String>,
        to_seat: usize,
    },
    PublicReveal {
        event_id: u64,
        player: usize,
        id: String,
    },
}

/// Public, non-secret card metadata that a [`DraftObservation`] may carry.
///
/// This mirrors the vocabulary the engine already exposes through
/// `Observation::card_definitions`: identity, kind, declared end-game points,
/// declared cost, and the *implemented* effect categories. It contains no
/// hidden location, hand, deck order, or seed information, and it never
/// invents an effect the card subsystem has not implemented (an unknown card
/// simply has no tags).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DraftCardInfo {
    pub id: String,
    pub kind: CardKind,
    pub points: i32,
    /// Declared cost as `(good, quantity)` with stable good names.
    #[serde(default)]
    pub cost: Vec<(String, u32)>,
    /// Implemented value-producing effect categories, e.g. `on_play_gain`.
    /// Empty means no implemented effect is known; the draft AI must not value
    /// such a card by guesswork.
    #[serde(default)]
    pub effect_tags: Vec<String>,
}

/// The only representation an AI or remote viewer may consume.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DraftObservation {
    pub viewer: usize,
    /// Public initial start-player seat, drawn before the draft begins.
    pub start_player: usize,
    pub mode: DraftMode,
    pub ruleset_id: String,
    pub phase: DraftPhase,
    pub round: u8,
    pub active_kind: Option<CardKind>,
    pub offered_occupation: Vec<String>,
    pub offered_minor: Vec<String>,
    pub own_picked: Vec<String>,
    pub own_observation_history: Vec<DraftHistoryEvent>,
    /// How many cards each seat has already picked. Public count, no identities.
    pub hand_counts: Vec<usize>,
    pub remaining_to_pick: u8,
    pub request_token: u64,
    /// Public card vocabulary for the offered and already-picked cards.
    /// Populated by [`DraftState::observe_with_catalog`]; empty otherwise, so
    /// policies fall back to legality rather than guessing card value.
    #[serde(default)]
    pub catalog: Vec<DraftCardInfo>,
}

impl DraftObservation {
    /// Public metadata for one card, if the coordinator supplied a catalog.
    pub fn card_info(&self, id: &str) -> Option<&DraftCardInfo> {
        self.catalog.iter().find(|info| info.id == id)
    }
}

/// Full private state. Serializing this is equivalent to a server-side save.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DraftState {
    pub schema_version: u32,
    pub mode: DraftMode,
    pub ruleset: DraftRuleset,
    pub player_count: usize,
    /// Public first start-player seat, chosen before the draft begins.
    pub start_player: usize,
    /// Server-only deck seed. Never placed in [`DraftObservation`].
    pub seed: u64,
    pub deck_version: String,
    pub round: u8,
    pub phase: DraftPhase,
    pub active_kind: Option<CardKind>,
    pub packets: Vec<DraftPacket>,
    /// Cards each seat has committed to its final hand, in pick order.
    pub picked: Vec<Vec<String>>,
    /// Private discards (e.g. the 3 unused cards in 10-7 modes).
    pub discarded: Vec<String>,
    pub commitments: Vec<Option<DraftCommit>>,
    pub history: Vec<Vec<DraftHistoryEvent>>,
    /// Which kinds have finished all seven 7-7 rounds (order-agnostic).
    #[serde(default)]
    pub drafted_kinds: Vec<CardKind>,
    /// Card identity lookup for this draft. Authoritative, not observed directly.
    pub kind_of: BTreeMap<String, CardKind>,
    pub actions: Vec<DraftAction>,
    pub event_index: u64,
}

impl DraftState {
    pub fn current_token(&self) -> u64 {
        self.event_index
    }

    pub fn is_complete(&self) -> bool {
        self.phase == DraftPhase::Complete
    }

    /// The final 7 + 7 hand per seat, only meaningful once complete.
    pub fn final_hands(&self) -> Vec<(Vec<String>, Vec<String>)> {
        self.picked
            .iter()
            .map(|ids| {
                (
                    ids.iter()
                        .filter(|id| self.card_kind(id) == Some(CardKind::Occupation))
                        .cloned()
                        .collect(),
                    ids.iter()
                        .filter(|id| self.card_kind(id) == Some(CardKind::MinorImprovement))
                        .cloned()
                        .collect(),
                )
            })
            .collect()
    }

    pub fn card_kind(&self, id: &str) -> Option<CardKind> {
        self.kind_of.get(id).copied()
    }

    fn packet_for(&self, seat: usize, kind: CardKind) -> Result<&DraftPacket, String> {
        self.packets
            .iter()
            .find(|p| p.owner_seat == seat && p.kind == kind)
            .ok_or_else(|| format!("no {} packet for seat {seat}", kind.as_str()))
    }

    fn packet_for_mut(&mut self, seat: usize, kind: CardKind) -> Result<&mut DraftPacket, String> {
        self.packets
            .iter_mut()
            .find(|p| p.owner_seat == seat && p.kind == kind)
            .ok_or_else(|| format!("no {} packet for seat {seat}", kind.as_str()))
    }

    /// The public part of the state, plus the viewer's own private slice.
    pub fn observe(&self, viewer: usize, request_token: u64) -> Result<DraftObservation, String> {
        if viewer >= self.player_count {
            return Err("invalid draft viewer".into());
        }
        let active = self.phase == DraftPhase::Drafting || self.phase == DraftPhase::Choosing;
        let offered = |kind: CardKind| {
            if active {
                self.packets
                    .iter()
                    .find(|p| p.owner_seat == viewer && p.kind == kind)
                    .map(|p| p.cards.clone())
                    .unwrap_or_default()
            } else {
                Vec::new()
            }
        };
        let remaining_to_pick = if self.is_complete() {
            0
        } else {
            match self.mode {
                DraftMode::Deal7 => 0,
                DraftMode::Choose10_7 => 7,
                _ => (self.mode.picks_per_kind() as u8).saturating_sub(self.round - 1),
            }
        };
        Ok(DraftObservation {
            viewer,
            start_player: self.start_player,
            mode: self.mode,
            ruleset_id: self.ruleset.id.clone(),
            phase: self.phase,
            round: self.round,
            active_kind: self.active_kind,
            offered_occupation: offered(CardKind::Occupation),
            offered_minor: offered(CardKind::MinorImprovement),
            own_picked: self.picked[viewer].clone(),
            own_observation_history: self.history[viewer].clone(),
            hand_counts: self.picked.iter().map(Vec::len).collect(),
            remaining_to_pick,
            request_token,
            catalog: Vec::new(),
        })
    }

    /// The authoritative observation plus the coordinator-supplied public card
    /// catalog. The catalog is public vocabulary (the same information as
    /// `Observation::card_definitions`), never hidden state, so attaching it
    /// keeps the "policies only see a `DraftObservation`" boundary intact.
    pub fn observe_with_catalog(
        &self,
        viewer: usize,
        request_token: u64,
        catalog: &[DraftCardInfo],
    ) -> Result<DraftObservation, String> {
        let mut observation = self.observe(viewer, request_token)?;
        observation.catalog = catalog.to_vec();
        Ok(observation)
    }

    /// Commit one pick for `viewer`. All seats must commit before the packets
    /// move, matching the design's atomic transfer rule. The whole operation is
    /// transactional: any error leaves `self` untouched.
    pub fn commit_pick(
        &mut self,
        viewer: usize,
        token: u64,
        occupation: Option<String>,
        minor: Option<String>,
    ) -> Result<(), String> {
        let mut next = self.clone();
        next.commit_pick_inner(viewer, token, occupation, minor)?;
        next.validate_invariants()?;
        *self = next;
        Ok(())
    }

    fn commit_pick_inner(
        &mut self,
        viewer: usize,
        token: u64,
        occupation: Option<String>,
        minor: Option<String>,
    ) -> Result<(), String> {
        self.check_token(token)?;
        if self.phase != DraftPhase::Drafting {
            return Err("draft is not in the picking phase".into());
        }
        if viewer >= self.player_count {
            return Err("invalid draft viewer".into());
        }
        if self.commitments[viewer].is_some() {
            return Err("seat already committed this round".into());
        }
        match self.mode {
            DraftMode::Draft77 => {
                let kind = self.active_kind.ok_or("missing active draft kind")?;
                let card = match kind {
                    CardKind::Occupation => {
                        if minor.is_some() {
                            return Err("unexpected minor pick in an occupation round".into());
                        }
                        occupation
                    }
                    CardKind::MinorImprovement => {
                        if occupation.is_some() {
                            return Err("unexpected occupation pick in a minor round".into());
                        }
                        minor
                    }
                };
                let card = card.ok_or("missing draft pick")?;
                if !self.packet_for(viewer, kind)?.cards.contains(&card) {
                    return Err(format!("card {card} is not offered to seat {viewer}"));
                }
                self.commitments[viewer] = Some(DraftCommit {
                    occupation: (kind == CardKind::Occupation).then(|| card.clone()),
                    minor: (kind == CardKind::MinorImprovement).then(|| card.clone()),
                });
            }
            DraftMode::Draft10_7Simultaneous => {
                let occ = occupation.ok_or("occupation pick is required")?;
                let min = minor.ok_or("minor pick is required")?;
                if !self
                    .packet_for(viewer, CardKind::Occupation)?
                    .cards
                    .contains(&occ)
                {
                    return Err(format!("occupation {occ} is not offered to seat {viewer}"));
                }
                if !self
                    .packet_for(viewer, CardKind::MinorImprovement)?
                    .cards
                    .contains(&min)
                {
                    return Err(format!("minor {min} is not offered to seat {viewer}"));
                }
                self.commitments[viewer] = Some(DraftCommit {
                    occupation: Some(occ),
                    minor: Some(min),
                });
            }
            _ => return Err("this mode does not use round picks".into()),
        }
        let committed = self.commitments[viewer].clone().expect("just set");
        self.actions.push(DraftAction::Pick {
            seat: viewer,
            occupation: committed.occupation,
            minor: committed.minor,
        });
        if self.commitments.iter().all(Option::is_some) {
            self.resolve_round()?;
        }
        Ok(())
    }

    /// Choose 7 of 10 for both kinds (10-7 non-draft mode). Transactional.
    pub fn commit_choice(
        &mut self,
        viewer: usize,
        token: u64,
        occupations: Vec<String>,
        minors: Vec<String>,
    ) -> Result<(), String> {
        let mut next = self.clone();
        next.commit_choice_inner(viewer, token, occupations, minors)?;
        next.validate_invariants()?;
        *self = next;
        Ok(())
    }

    fn commit_choice_inner(
        &mut self,
        viewer: usize,
        token: u64,
        occupations: Vec<String>,
        minors: Vec<String>,
    ) -> Result<(), String> {
        self.check_token(token)?;
        if self.mode != DraftMode::Choose10_7 || self.phase != DraftPhase::Choosing {
            return Err("draft is not in the choosing phase".into());
        }
        if viewer >= self.player_count {
            return Err("invalid draft viewer".into());
        }
        if self.commitments[viewer].is_some() {
            return Err("seat already chose its hand".into());
        }
        validate_choice(self.packet_for(viewer, CardKind::Occupation)?, &occupations)?;
        validate_choice(
            self.packet_for(viewer, CardKind::MinorImprovement)?,
            &minors,
        )?;
        let leftover_occ = self
            .packet_for(viewer, CardKind::Occupation)?
            .cards
            .iter()
            .filter(|id| !occupations.contains(id))
            .cloned()
            .collect::<Vec<_>>();
        let leftover_minor = self
            .packet_for(viewer, CardKind::MinorImprovement)?
            .cards
            .iter()
            .filter(|id| !minors.contains(id))
            .cloned()
            .collect::<Vec<_>>();
        for id in &occupations {
            self.history[viewer].push(DraftHistoryEvent::OwnPick {
                round: 0,
                kind: CardKind::Occupation,
                id: id.clone(),
            });
        }
        for id in &minors {
            self.history[viewer].push(DraftHistoryEvent::OwnPick {
                round: 0,
                kind: CardKind::MinorImprovement,
                id: id.clone(),
            });
        }
        self.picked[viewer] = occupations.iter().chain(minors.iter()).cloned().collect();
        self.discarded.extend(leftover_occ);
        self.discarded.extend(leftover_minor);
        self.packet_for_mut(viewer, CardKind::Occupation)?
            .cards
            .clear();
        self.packet_for_mut(viewer, CardKind::MinorImprovement)?
            .cards
            .clear();
        self.commitments[viewer] = Some(DraftCommit {
            occupation: None,
            minor: None,
        });
        self.actions.push(DraftAction::Choice {
            seat: viewer,
            occupations,
            minors,
        });
        self.event_index += 1;
        if self.commitments.iter().all(Option::is_some) {
            self.phase = DraftPhase::Complete;
        }
        Ok(())
    }

    fn check_token(&self, token: u64) -> Result<(), String> {
        if token != self.event_index {
            return Err("stale or duplicate draft request token".into());
        }
        Ok(())
    }

    fn resolve_round(&mut self) -> Result<(), String> {
        let mut selections = Vec::new();
        for (seat, commit) in self.commitments.iter().enumerate() {
            let commit = commit.as_ref().expect("all committed");
            if let Some(id) = &commit.occupation {
                selections.push((seat, CardKind::Occupation, id.clone()));
            }
            if let Some(id) = &commit.minor {
                selections.push((seat, CardKind::MinorImprovement, id.clone()));
            }
        }
        for (seat, kind, id) in selections {
            let packet = self.packet_for_mut(seat, kind)?;
            let position = packet
                .cards
                .iter()
                .position(|c| c == &id)
                .ok_or_else(|| format!("card {id} is not in seat {seat}'s packet"))?;
            packet.cards.remove(position);
            self.picked[seat].push(id.clone());
            self.history[seat].push(DraftHistoryEvent::OwnPick {
                round: self.round,
                kind,
                id,
            });
        }
        match self.mode {
            DraftMode::Draft77 => {
                let kind = self.active_kind.ok_or("missing active draft kind")?;
                if self.round < 7 {
                    self.pass_packets(kind)?;
                }
                self.commitments = vec![None; self.player_count];
                if self.round >= 7 {
                    self.drafted_kinds.push(kind);
                    let other = match kind {
                        CardKind::Occupation => CardKind::MinorImprovement,
                        CardKind::MinorImprovement => CardKind::Occupation,
                    };
                    if self.drafted_kinds.contains(&other) {
                        self.phase = DraftPhase::Complete;
                    } else {
                        self.active_kind = Some(other);
                        self.round = 1;
                    }
                } else {
                    self.round += 1;
                }
            }
            DraftMode::Draft10_7Simultaneous => {
                if self.round < 7 {
                    self.pass_packets(CardKind::Occupation)?;
                    self.pass_packets(CardKind::MinorImprovement)?;
                } else {
                    let mut leftovers = Vec::new();
                    for packet in self.packets.iter_mut() {
                        leftovers.append(&mut packet.cards);
                    }
                    self.discarded.append(&mut leftovers);
                }
                self.commitments = vec![None; self.player_count];
                if self.round >= 7 {
                    self.phase = DraftPhase::Complete;
                } else {
                    self.round += 1;
                }
            }
            _ => return Err("unexpected resolve in this mode".into()),
        }
        self.event_index += 1;
        Ok(())
    }

    /// Move every packet of one kind to its neighbour, recording what each
    /// passer saw it pass and what each receiver now sees.
    fn pass_packets(&mut self, kind: CardKind) -> Result<(), String> {
        let direction = self.ruleset.direction(kind);
        let players = self.player_count;
        let round = self.round;
        let mut moves = Vec::new();
        for packet in self.packets.iter().filter(|p| p.kind == kind) {
            let from = packet.owner_seat;
            let to = direction.step(from, players);
            moves.push((packet.packet_id, from, to, packet.cards.clone()));
        }
        for (packet_id, from, to, remaining) in moves {
            self.history[from].push(DraftHistoryEvent::PassedPacket {
                round,
                kind,
                ids: remaining.clone(),
                to_seat: to,
            });
            if let Some(packet) = self.packets.iter_mut().find(|p| p.packet_id == packet_id) {
                packet.owner_seat = to;
            }
            self.history[to].push(DraftHistoryEvent::SeenPacket {
                round: round + 1,
                kind,
                ids: remaining,
                packet_id,
            });
        }
        Ok(())
    }

    /// Full authoritative-state hash (includes private data and the seed).
    pub fn state_hash(&self) -> String {
        let data = serde_json::to_vec(&(
            self.schema_version,
            self.mode,
            &self.ruleset,
            self.player_count,
            (self.start_player, self.seed),
            &self.deck_version,
            self.round,
            self.phase,
            self.active_kind,
            &self.packets,
            &self.picked,
            &self.discarded,
            &self.commitments,
            (&self.history, &self.drafted_kinds),
            &self.kind_of,
            self.event_index,
        ))
        .expect("draft state serialization");
        format!("{:x}", Sha256::digest(&data))
    }

    /// Server-side complete-log hash. Kept separate from the observable view.
    pub fn log_hash(&self) -> String {
        let data = serde_json::to_vec(&(
            self.seed,
            self.start_player,
            &self.deck_version,
            &self.actions,
            &self.history,
        ))
        .expect("draft log serialization");
        format!("{:x}", Sha256::digest(&data))
    }

    /// Structural invariants for every state change.
    pub fn validate_invariants(&self) -> Result<(), String> {
        if self.schema_version != DRAFT_SCHEMA_VERSION {
            return Err("draft schema version mismatch".into());
        }
        if !(2..=4).contains(&self.player_count) {
            return Err("draft player_count out of range".into());
        }
        if self.start_player >= self.player_count {
            return Err("draft initial start player out of range".into());
        }
        if self.picked.len() != self.player_count
            || self.commitments.len() != self.player_count
            || self.history.len() != self.player_count
        {
            return Err("draft per-seat vectors have the wrong length".into());
        }
        // Each kind has exactly one packet per seat, including after completion
        // (when packets are empty). Detect duplicated owners and forged packet IDs.
        let expected_packets = 2 * self.player_count;
        if self.packets.len() != expected_packets {
            return Err("draft packet count does not match player_count".into());
        }
        let mut packet_ids = BTreeSet::new();
        let mut packet_owners = BTreeSet::new();
        for packet in &self.packets {
            if packet.packet_id as usize >= expected_packets || !packet_ids.insert(packet.packet_id)
            {
                return Err("draft packet IDs are missing, duplicated or out of range".into());
            }
            if packet.owner_seat >= self.player_count
                || !packet_owners.insert((packet.owner_seat, packet.kind))
            {
                return Err("draft has invalid or duplicated packet owner/kind".into());
            }
        }

        // Cards in each draft are conserved from their initial deal: exactly
        // dealt_per_kind * players of each kind must exist in packets, picked,
        // or private discards. Unused pool cards are intentionally not counted.
        let mut seen = BTreeSet::new();
        let mut counts = BTreeMap::<CardKind, usize>::new();
        for packet in &self.packets {
            for id in &packet.cards {
                if self.card_kind(id) != Some(packet.kind) {
                    return Err(format!("card {id} has the wrong packet kind"));
                }
            }
        }
        for id in self
            .packets
            .iter()
            .flat_map(|p| p.cards.iter())
            .chain(self.picked.iter().flatten())
            .chain(self.discarded.iter())
        {
            let kind = self
                .card_kind(id)
                .ok_or_else(|| format!("unknown draft card: {id}"))?;
            if !seen.insert(id.clone()) {
                return Err(format!("draft card appears twice: {id}"));
            }
            *counts.entry(kind).or_default() += 1;
        }
        let expected_kind_cards = self.mode.dealt_per_kind() * self.player_count;
        for kind in [CardKind::Occupation, CardKind::MinorImprovement] {
            if counts.get(&kind).copied().unwrap_or_default() != expected_kind_cards {
                return Err(format!("{} draft cards were lost or added", kind.as_str()));
            }
        }
        for ids in self.picked.iter() {
            let occ = ids
                .iter()
                .filter(|id| self.card_kind(id) == Some(CardKind::Occupation))
                .count();
            let minor = ids
                .iter()
                .filter(|id| self.card_kind(id) == Some(CardKind::MinorImprovement))
                .count();
            if occ > self.mode.final_hand_per_kind() || minor > self.mode.final_hand_per_kind() {
                return Err("seat picked too many cards".into());
            }
        }
        if self.phase == DraftPhase::Complete {
            for (seat, ids) in self.picked.iter().enumerate() {
                let occ = ids
                    .iter()
                    .filter(|id| self.card_kind(id) == Some(CardKind::Occupation))
                    .count();
                let minor = ids
                    .iter()
                    .filter(|id| self.card_kind(id) == Some(CardKind::MinorImprovement))
                    .count();
                if occ != self.mode.final_hand_per_kind()
                    || minor != self.mode.final_hand_per_kind()
                {
                    return Err(format!("seat {seat} final hand is not 7 + 7"));
                }
            }
        }
        Ok(())
    }
}

/// Rebuild a draft from the same setup and the recorded actions, then return
/// the replayed state. Callers compare [`DraftState::state_hash`].
pub fn replay_draft(setup: &DraftSetup, actions: &[DraftAction]) -> Result<DraftState, String> {
    let mut state = setup.start()?;
    for action in actions {
        let token = state.event_index;
        match action {
            DraftAction::Pick {
                seat,
                occupation,
                minor,
            } => state.commit_pick(*seat, token, occupation.clone(), minor.clone())?,
            DraftAction::Choice {
                seat,
                occupations,
                minors,
            } => state.commit_choice(*seat, token, occupations.clone(), minors.clone())?,
        }
    }
    Ok(state)
}

/// Deterministic CPU pick that reads only the viewer's own observation.
pub fn cpu_pick(obs: &DraftObservation) -> Result<DraftCommit, String> {
    if obs.phase != DraftPhase::Drafting {
        return Err("CPU is not in a picking phase".into());
    }
    let (occupation, minor) = match obs.mode {
        DraftMode::Draft77 => match obs.active_kind {
            Some(CardKind::Occupation) => (obs.offered_occupation.first().cloned(), None),
            Some(CardKind::MinorImprovement) => (None, obs.offered_minor.first().cloned()),
            None => return Err("CPU draft round has no active kind".into()),
        },
        DraftMode::Draft10_7Simultaneous => (
            obs.offered_occupation.first().cloned(),
            obs.offered_minor.first().cloned(),
        ),
        _ => return Err("CPU pick not supported in this mode".into()),
    };
    if occupation.is_none() && minor.is_none() {
        return Err("CPU has no offered card".into());
    }
    Ok(DraftCommit { occupation, minor })
}

/// Deterministic CPU 7-of-10 choice from the viewer's own observation only.
pub fn cpu_choice(obs: &DraftObservation) -> Result<(Vec<String>, Vec<String>), String> {
    if obs.mode != DraftMode::Choose10_7 || obs.phase != DraftPhase::Choosing {
        return Err("CPU choice not supported in this mode".into());
    }
    let occupations = obs
        .offered_occupation
        .iter()
        .take(7)
        .cloned()
        .collect::<Vec<_>>();
    let minors = obs
        .offered_minor
        .iter()
        .take(7)
        .cloned()
        .collect::<Vec<_>>();
    if occupations.len() != 7 || minors.len() != 7 {
        return Err("CPU choice needs at least 7 offered cards per kind".into());
    }
    Ok((occupations, minors))
}

fn validate_choice(packet: &DraftPacket, chosen: &[String]) -> Result<(), String> {
    if chosen.len() != 7 {
        return Err(format!(
            "{} choice must select exactly 7 cards, got {}",
            packet.kind.as_str(),
            chosen.len()
        ));
    }
    let offered: BTreeSet<&String> = packet.cards.iter().collect();
    let mut unique = BTreeSet::new();
    for id in chosen {
        if !offered.contains(id) {
            return Err(format!("card {id} is not offered in this packet"));
        }
        if !unique.insert(id) {
            return Err(format!("duplicate card in choice: {id}"));
        }
    }
    Ok(())
}

fn shuffle(ids: &[String], seed: u64, salt: u64) -> Vec<String> {
    let mut rng = ChaCha8Rng::seed_from_u64(seed ^ salt);
    let mut deck = ids.to_vec();
    deck.shuffle(&mut rng);
    deck
}
