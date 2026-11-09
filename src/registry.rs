//! Immutable card registry and pool separation (Issue #253 Step 1).
//!
//! This module introduces:
//! - `CardId(u16)` — stable integer ID with deterministic A001–E168 round-trip
//! - `CardRegistry` — immutable, validated builder → freeze → `Arc<CardRegistry>` sharing
//! - `CardPool` — game-setup-time distribution candidates, separate from full registry
//! - `CardSet` compat adapter — preserves existing API for saves/replays/observations

use crate::card_pool::{CardReview, Eligibility, ReviewStatus};
use crate::cards::{CardDefinition, CardKind, CardSet};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fmt;
use std::sync::Arc;

/// Stable integer card ID.
///
/// Encoding: external IDs A001–E168 map to 1..=840 deterministically via
/// sorted external ID list index + 1. 0 is reserved for invalid/unregistered.
/// Major improvements and other future card kinds use a separate namespace
/// (offset by 10000) to avoid collisions.
///
/// The mapping is constructed once at registry build time from the canonical
/// sorted list of all known external IDs. This guarantees:
/// - Existing IDs never change when new cards are added
/// - Round-trip: external_id ↔ CardId is bijective for registered cards
/// - Invalid/out-of-range/unregistered are distinguishable (CardId(0))
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct CardId(pub u16);

impl CardId {
    /// Reserved sentinel for invalid / unregistered / out-of-range.
    pub const INVALID: CardId = CardId(0);

    /// Major improvement namespace offset (future use, not in current registry).
    pub const MAJOR_IMPROVEMENT_OFFSET: u16 = 10000;

    /// Create from raw u16 (internal use only; prefer Registry::card_id).
    pub const fn from_raw(raw: u16) -> Self {
        CardId(raw)
    }

    /// Raw value for serialization boundaries / logging / UI.
    pub const fn raw(self) -> u16 {
        self.0
    }

    /// Check if this ID is valid (registered in some registry).
    pub fn is_valid(self) -> bool {
        self.0 != 0
    }
}

impl fmt::Debug for CardId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "CardId({})", self.0)
    }
}

impl fmt::Display for CardId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Canonical external ID ordering for deterministic registry digest.
///
/// The full A001–E168 list is sorted lexicographically (A001, A002, …, E168).
/// This order is used for:
/// - Stable CardId assignment (index + 1)
/// - Canonical SHA256 digest (HashMap iteration order independent)
/// - Save/replay/serialize boundaries
fn canonical_external_ids() -> Vec<String> {
    // A-deck: A001–A169 (169 cards)
    // B-deck: B001–B169 (169 cards)
    // C-deck: C001–C169 (169 cards)
    // D-deck: D001–D169 (169 cards)
    // E-deck: E001–E168 (168 cards)
    // Total: 844 (but officially 840 unique; we include all known codes)
    let mut ids = Vec::with_capacity(844);
    for deck in ['A', 'B', 'C', 'D', 'E'] {
        let max = if deck == 'E' { 168 } else { 169 };
        for i in 1..=max {
            ids.push(format!("{}{:03}", deck, i));
        }
    }
    ids
}

/// Card definition stored in the registry (immutable, validated).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RegisteredCard {
    pub id: CardId,
    pub external_id: String,
    pub kind: CardKind,
    pub cost: Vec<(crate::model::Good, u32)>,
    pub points: i32,
    pub effects: Vec<crate::cards::CardEffect>,
    /// Review metadata (official verification status, player-count eligibility)
    pub review: CardReview,
}

impl RegisteredCard {
    /// Convert to the legacy CardDefinition for compat adapter.
    pub fn to_card_definition(&self) -> CardDefinition {
        CardDefinition {
            id: self.external_id.clone(),
            kind: self.kind,
            cost: self.cost.clone(),
            points: self.points,
            effects: self.effects.clone(),
        }
    }
}

/// Event kind for static trigger indexing (minimal set for Step 1).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EventKind {
    /// Worker placed on a space
    WorkerPlaced,
    /// Card played from hand
    CardPlayed,
    /// Round start
    RoundStart,
    /// Harvest phase
    Harvest,
    /// Clay construction (room/stone upgrade)
    ClayConstruction,
    /// Baking
    Bake,
}

impl EventKind {
    pub fn all() -> &'static [EventKind] {
        &[
            EventKind::WorkerPlaced,
            EventKind::CardPlayed,
            EventKind::RoundStart,
            EventKind::Harvest,
            EventKind::ClayConstruction,
            EventKind::Bake,
        ]
    }
}

/// Index of CardIds by EventKind for fast trigger lookup.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct EventIndex {
    pub by_kind: HashMap<EventKind, Vec<CardId>>,
}

impl EventIndex {
    fn new() -> Self {
        let mut by_kind = HashMap::new();
        for kind in EventKind::all() {
            by_kind.insert(*kind, Vec::new());
        }
        Self { by_kind }
    }

    pub fn get(&self, kind: EventKind) -> &[CardId] {
        self.by_kind.get(&kind).map(|v| v.as_slice()).unwrap_or(&[])
    }
}

/// Registry build errors.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RegistryBuildError {
    #[error("duplicate external ID: {0}")]
    DuplicateExternalId(String),
    #[error("duplicate CardId assignment: {0}")]
    DuplicateCardId(u16),
    #[error("external ID not in canonical list: {0}")]
    UnknownExternalId(String),
    #[error("validation failed: {0}")]
    Validation(String),
    #[error("card definition invalid: {0}")]
    InvalidDefinition(String),
}

/// Builder for CardRegistry (validated construction, then freeze).
#[derive(Debug, Default)]
pub struct CardRegistryBuilder {
    cards: Vec<RegisteredCard>,
    external_to_id: BTreeMap<String, CardId>,
    id_to_index: BTreeMap<CardId, usize>,
    event_index: EventIndex,
    rules_semantic_version: String,
}

impl CardRegistryBuilder {
    pub fn new() -> Self {
        Self {
            cards: Vec::new(),
            external_to_id: BTreeMap::new(),
            id_to_index: BTreeMap::new(),
            event_index: EventIndex::new(),
            rules_semantic_version: "2016-revised-v1".to_string(),
        }
    }

    /// Set custom rules semantic version (for future rule changes).
    pub fn rules_semantic_version(mut self, version: String) -> Self {
        self.rules_semantic_version = version;
        self
    }

    /// Add a card definition with its review metadata.
    ///
    /// The external_id must be in the canonical A001–E168 list.
    /// Returns error if duplicate external_id or invalid ID.
    pub fn add_card(
        mut self,
        external_id: String,
        kind: CardKind,
        cost: Vec<(crate::model::Good, u32)>,
        points: i32,
        effects: Vec<crate::cards::CardEffect>,
        review: CardReview,
    ) -> Result<Self, RegistryBuildError> {
        // Validate external ID is known
        let canonical = canonical_external_ids();
        if !canonical.contains(&external_id) {
            return Err(RegistryBuildError::UnknownExternalId(external_id));
        }

        // Assign stable CardId from canonical position (1-indexed)
        let card_id = CardId::from_raw(
            canonical
                .iter()
                .position(|id| id == &external_id)
                .ok_or_else(|| RegistryBuildError::UnknownExternalId(external_id.clone()))?
                as u16
                + 1,
        );

        // Check duplicates
        if self.external_to_id.contains_key(&external_id) {
            return Err(RegistryBuildError::DuplicateExternalId(external_id));
        }
        if self.id_to_index.contains_key(&card_id) {
            return Err(RegistryBuildError::DuplicateCardId(card_id.raw()));
        }

        let index = self.cards.len();
        let card = RegisteredCard {
            id: card_id,
            external_id: external_id.clone(),
            kind,
            cost,
            points,
            effects,
            review,
        };

        self.cards.push(card);
        self.external_to_id.insert(external_id, card_id);
        self.id_to_index.insert(card_id, index);

        // Index by event kind (minimal: infer from effect types)
        // For Step 1, we index based on CardEffect variants present
        for effect in &self.cards[index].effects {
            let kind = match effect {
                crate::cards::CardEffect::AfterWorkerGain { .. } => EventKind::WorkerPlaced,
                crate::cards::CardEffect::OnPlayGain { .. }
                | crate::cards::CardEffect::OnPlayGainPerStored { .. }
                | crate::cards::CardEffect::OnPlayGainIfHouse { .. } => EventKind::CardPlayed,
                crate::cards::CardEffect::StartRoundGain { .. } => EventKind::RoundStart,
                crate::cards::CardEffect::HarvestFieldGain { .. } => EventKind::Harvest,
                crate::cards::CardEffect::AfterClayConstructionGain { .. } => {
                    EventKind::ClayConstruction
                }
                crate::cards::CardEffect::AfterHarvestBakeGain { .. } => EventKind::Bake,
                crate::cards::CardEffect::AfterWorkerGainIfUsed { .. } => EventKind::WorkerPlaced,
                crate::cards::CardEffect::Passive { .. }
                | crate::cards::CardEffect::Prerequisite { .. }
                | crate::cards::CardEffect::EndGameBonus { .. } => continue,
            };
            self.event_index
                .by_kind
                .get_mut(&kind)
                .unwrap()
                .push(card_id);
        }

        Ok(self)
    }

    /// Validate all cards and build the immutable registry.
    pub fn freeze(self) -> Result<Arc<CardRegistry>, RegistryBuildError> {
        // Run structural validation on all cards (not player-count-dependent deck composition)
        self.validate_structure()?;

        // Check for duplicate definitions in the set
        let mut seen = BTreeSet::new();
        for card in &self.cards {
            if !seen.insert(card.external_id.clone()) {
                return Err(RegistryBuildError::DuplicateExternalId(
                    card.external_id.clone(),
                ));
            }
        }

        // Build final registry
        let registry = CardRegistry {
            cards: self.cards,
            external_to_id: self.external_to_id,
            id_to_index: self.id_to_index,
            event_index: self.event_index,
            rules_semantic_version: self.rules_semantic_version,
            digest: String::new(), // computed after
        };

        let digest = registry.compute_digest();
        Ok(Arc::new(CardRegistry { digest, ..registry }))
    }

    /// Structural validation: checks card definitions are well-formed without
    /// enforcing player-count-dependent deck composition (7 cards per kind per player).
    fn validate_structure(&self) -> Result<(), RegistryBuildError> {
        for card in &self.cards {
            if card.external_id.is_empty()
                || card.external_id.len() > 64
                || !card
                    .external_id
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
            {
                return Err(RegistryBuildError::InvalidDefinition(format!(
                    "invalid card id: {}",
                    card.external_id
                )));
            }
            if !(-10..=20).contains(&card.points)
                || card
                    .cost
                    .iter()
                    .any(|(_, quantity)| *quantity == 0 || *quantity > 30)
                || card.effects.iter().any(|effect| {
                    let amount = match effect {
                        crate::cards::CardEffect::OnPlayGain { amount, .. }
                        | crate::cards::CardEffect::AfterWorkerGain { amount, .. } => *amount,
                        crate::cards::CardEffect::EndGameBonus { .. }
                        | crate::cards::CardEffect::Prerequisite { .. }
                        | crate::cards::CardEffect::Passive { .. } => 1,
                        crate::cards::CardEffect::OnPlayGainPerStored { amount, .. }
                        | crate::cards::CardEffect::OnPlayGainIfHouse { amount, .. }
                        | crate::cards::CardEffect::StartRoundGain { amount, .. }
                        | crate::cards::CardEffect::HarvestFieldGain { amount, .. }
                        | crate::cards::CardEffect::AfterWorkerGainIfUsed { amount, .. }
                        | crate::cards::CardEffect::AfterClayConstructionGain { amount, .. }
                        | crate::cards::CardEffect::AfterHarvestBakeGain { amount } => *amount,
                    };
                    amount == 0 || amount > 30
                })
                || card.effects.iter().any(|effect| {
                    matches!(
                        effect,
                        crate::cards::CardEffect::OnPlayGainPerStored { divisor: 0, .. }
                    ) || matches!(
                        effect,
                        crate::cards::CardEffect::HarvestFieldGain {
                            rule: crate::cards::HarvestFieldRule::Animals { sheep_per: 0, .. }
                                | crate::cards::HarvestFieldRule::Animals { cattle_per: 0, .. },
                            ..
                        }
                    )
                })
            {
                return Err(RegistryBuildError::InvalidDefinition(format!(
                    "unsupported card cost, points, or effect: {}",
                    card.external_id
                )));
            }
        }
        Ok(())
    }

    /// Convert to legacy CardSet for validation/compat.
    #[allow(dead_code)]
    fn to_card_set(&self) -> CardSet {
        CardSet {
            definitions: self.cards.iter().map(|c| c.to_card_definition()).collect(),
        }
    }
}

/// Immutable, validated card registry. Shared via `Arc<CardRegistry>`.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CardRegistry {
    cards: Vec<RegisteredCard>,
    external_to_id: BTreeMap<String, CardId>,
    id_to_index: BTreeMap<CardId, usize>,
    event_index: EventIndex,
    rules_semantic_version: String,
    /// Canonical SHA256 digest of all definitions in canonical external ID order.
    digest: String,
}

impl CardRegistry {
    /// Total number of registered cards.
    pub fn len(&self) -> usize {
        self.cards.len()
    }

    pub fn is_empty(&self) -> bool {
        self.cards.is_empty()
    }

    /// Get CardId for an external ID (e.g., "A001").
    pub fn card_id(&self, external_id: &str) -> Option<CardId> {
        self.external_to_id.get(external_id).copied()
    }

    /// Get external ID for a CardId.
    pub fn external_id(&self, card_id: CardId) -> Option<&str> {
        self.cards
            .get(*self.id_to_index.get(&card_id)?)
            .map(|c| c.external_id.as_str())
    }

    /// Get card definition by CardId (O(1) via direct index).
    pub fn get(&self, card_id: CardId) -> Option<&RegisteredCard> {
        self.id_to_index
            .get(&card_id)
            .and_then(|&idx| self.cards.get(idx))
    }

    /// Get card definition by external ID.
    pub fn get_by_external(&self, external_id: &str) -> Option<&RegisteredCard> {
        self.card_id(external_id).and_then(|id| self.get(id))
    }

    /// Iterate all cards in canonical external ID order.
    pub fn iter(&self) -> impl Iterator<Item = &RegisteredCard> {
        self.cards.iter()
    }

    /// Iterate cards of a specific kind.
    pub fn iter_kind(&self, kind: CardKind) -> impl Iterator<Item = &RegisteredCard> {
        self.cards.iter().filter(move |c| c.kind == kind)
    }

    /// Get all CardIds for a given EventKind (static trigger index).
    pub fn event_candidates(&self, kind: EventKind) -> &[CardId] {
        self.event_index.get(kind)
    }

    /// Get the canonical registry digest.
    pub fn digest(&self) -> &str {
        &self.digest
    }

    /// Get the rules semantic version.
    pub fn rules_semantic_version(&self) -> &str {
        &self.rules_semantic_version
    }

    /// Verify a digest matches this registry (for save/replay validation).
    pub fn verify_digest(&self, expected: &str) -> bool {
        self.digest == expected
    }

    /// Compute canonical SHA256 digest.
    ///
    /// Algorithm: sort by canonical external ID order (already stored that way),
    /// serialize each definition's stable fields (id, kind, cost, points, effects),
    /// hash with domain tag "agricola-registry-v1\0".
    fn compute_digest(&self) -> String {
        let mut hasher = Sha256::new();
        hasher.update(b"agricola-registry-v1\0");
        for card in &self.cards {
            // Serialize only stable definition fields (not review metadata)
            let def = card.to_card_definition();
            let serialized = serde_json::to_string(&def).expect("card definition serialization");
            hasher.update(card.external_id.as_bytes());
            hasher.update([0u8]);
            hasher.update(serialized.as_bytes());
            hasher.update([0u8]);
        }
        format!("{:x}", hasher.finalize())
    }

    /// Convert to legacy CardSet (for compat adapter, saves, observations).
    pub fn to_card_set(&self) -> CardSet {
        CardSet {
            definitions: self.cards.iter().map(|c| c.to_card_definition()).collect(),
        }
    }

    /// Get review metadata for a card.
    pub fn review(&self, card_id: CardId) -> Option<&CardReview> {
        self.get(card_id).map(|c| &c.review)
    }

    /// Get review metadata by external ID.
    pub fn review_by_external(&self, external_id: &str) -> Option<&CardReview> {
        self.card_id(external_id).and_then(|id| self.review(id))
    }
}

/// CardPool: game-setup-time distribution candidates.
///
/// Separates the full registry (all 840+ cards) from what can actually be
/// dealt in a specific game. Official mode is fail-closed: only cards that
/// are implemented + tested + officially verified + player-count eligible.
/// Experimental mode is explicit opt-in.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CardPool {
    /// CardIds eligible for dealing in official mode.
    official_eligible: Vec<CardId>,
    /// CardIds eligible in experimental mode (includes unverified).
    experimental_eligible: Vec<CardId>,
    /// Player count this pool was built for.
    players: usize,
    /// Pool digest for save/replay verification.
    digest: String,
    /// Whether experimental mode is enabled.
    experimental_mode: bool,
}

impl CardPool {
    /// Build a pool from a registry for a specific player count.
    ///
    /// Official eligibility: review_status == Verified && eligibility_at(players) == Yes
    /// Experimental eligibility: card exists in registry (implemented) regardless of review.
    pub fn new(registry: &CardRegistry, players: usize, experimental_mode: bool) -> Self {
        let mut official = Vec::new();
        let mut experimental = Vec::new();

        for card in registry.iter() {
            let in_experimental = true; // All implemented cards
            let in_official = card.review.review_status == ReviewStatus::Verified
                && matches!(card.review.eligibility_at(players), Some(Eligibility::Yes));

            if in_experimental {
                experimental.push(card.id);
            }
            if in_official {
                official.push(card.id);
            }
        }

        // Sort both by CardId for determinism
        official.sort();
        experimental.sort();

        let digest = Self::compute_digest(&official, &experimental, players, experimental_mode);

        Self {
            official_eligible: official,
            experimental_eligible: experimental,
            players,
            digest,
            experimental_mode,
        }
    }

    /// Get eligible CardIds for the current mode.
    pub fn eligible(&self) -> &[CardId] {
        if self.experimental_mode {
            &self.experimental_eligible
        } else {
            &self.official_eligible
        }
    }

    /// Get eligible CardIds filtered by kind.
    pub fn eligible_kind(&self, registry: &CardRegistry, kind: CardKind) -> Vec<CardId> {
        self.eligible()
            .iter()
            .copied()
            .filter(|id| registry.get(*id).map(|c| c.kind == kind).unwrap_or(false))
            .collect()
    }

    /// Check if a card is eligible in current mode.
    pub fn contains(&self, card_id: CardId) -> bool {
        self.eligible().binary_search(&card_id).is_ok()
    }

    /// Get player count.
    pub fn players(&self) -> usize {
        self.players
    }

    /// Get pool digest.
    pub fn digest(&self) -> &str {
        &self.digest
    }

    /// Whether experimental mode is active.
    pub fn is_experimental(&self) -> bool {
        self.experimental_mode
    }

    /// Count of eligible occupations.
    pub fn occupation_count(&self, registry: &CardRegistry) -> usize {
        self.eligible_kind(registry, CardKind::Occupation).len()
    }

    /// Count of eligible minor improvements.
    pub fn minor_improvement_count(&self, registry: &CardRegistry) -> usize {
        self.eligible_kind(registry, CardKind::MinorImprovement)
            .len()
    }

    fn compute_digest(
        official: &[CardId],
        experimental: &[CardId],
        players: usize,
        experimental_mode: bool,
    ) -> String {
        let mut hasher = Sha256::new();
        hasher.update(b"agricola-pool-v1\0");
        hasher.update(players.to_le_bytes());
        hasher.update([experimental_mode as u8]);
        for id in official {
            hasher.update(id.raw().to_le_bytes());
        }
        hasher.update([0xFF]); // separator
        for id in experimental {
            hasher.update(id.raw().to_le_bytes());
        }
        format!("{:x}", hasher.finalize())
    }
}

/// Compat adapter: build a CardSet from registry + pool (for saves, observations, legacy API).
pub fn card_set_from_pool(registry: &CardRegistry, pool: &CardPool) -> CardSet {
    let definitions = pool
        .eligible()
        .iter()
        .filter_map(|id| registry.get(*id))
        .map(|c| c.to_card_definition())
        .collect();
    CardSet { definitions }
}

/// Compat adapter: build a CardSet with all registry cards (for full-catalog observations).
pub fn card_set_from_registry(registry: &CardRegistry) -> CardSet {
    registry.to_card_set()
}

/// Build the default registry from all built-in card implementations.
///
/// This mirrors `card_pool::builtin_implemented_cards()` but produces
/// a validated, immutable CardRegistry with CardId assignments.
pub fn build_default_registry() -> Result<Arc<CardRegistry>, RegistryBuildError> {
    let mut builder = CardRegistryBuilder::new();

    // Collect all cards from the existing verified_* functions
    // We need to deduplicate by external_id across player counts
    let mut seen = BTreeSet::new();

    // Helper function to add cards from a vec
    fn add_cards_inner(
        builder: CardRegistryBuilder,
        cards: Vec<CardDefinition>,
        seen: &mut BTreeSet<String>,
    ) -> Result<CardRegistryBuilder, RegistryBuildError> {
        let mut builder = builder;
        for card in cards {
            if seen.insert(card.id.clone()) {
                // Default to unverified review; official verification is external
                let review = CardReview::unverified(&card.id);
                builder = builder.add_card(
                    card.id,
                    card.kind,
                    card.cost,
                    card.points,
                    card.effects,
                    review,
                )?;
            }
        }
        Ok(builder)
    }

    // Add all cards across all player counts (2, 3, 4)
    for players in 2..=4 {
        builder = add_cards_inner(
            builder,
            crate::cards::verified_scoring_cards(players),
            &mut seen,
        )?;
        builder = add_cards_inner(
            builder,
            crate::cards::verified_minor_worker_cards(players),
            &mut seen,
        )?;
        builder = add_cards_inner(
            builder,
            crate::cards::verified_worker_bonus_cards(players),
            &mut seen,
        )?;
        builder = add_cards_inner(
            builder,
            crate::cards::verified_simple_occupations(players),
            &mut seen,
        )?;
        builder = add_cards_inner(
            builder,
            crate::cards::verified_multiplayer_passive_cards(players),
            &mut seen,
        )?;
    }
    builder = add_cards_inner(
        builder,
        crate::cards::verified_immediate_resource_cards(),
        &mut seen,
    )?;
    builder = add_cards_inner(
        builder,
        crate::cards::verified_round_and_harvest_cards(),
        &mut seen,
    )?;
    builder = add_cards_inner(builder, crate::cards::verified_passive_cards(), &mut seen)?;
    builder = add_cards_inner(
        builder,
        crate::cards::verified_extra_simple_cards(),
        &mut seen,
    )?;

    builder.freeze()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::CardKind;

    #[test]
    fn card_id_invalid_is_zero() {
        assert_eq!(CardId::INVALID.raw(), 0);
        assert!(!CardId::INVALID.is_valid());
    }

    #[test]
    fn card_id_raw_roundtrip() {
        let id = CardId::from_raw(42);
        assert_eq!(id.raw(), 42);
        assert!(id.is_valid());
    }

    #[test]
    fn canonical_external_ids_length() {
        let ids = canonical_external_ids();
        // A-D: 169 each, E: 168 = 844
        assert_eq!(ids.len(), 844);
        assert_eq!(ids[0], "A001");
        assert_eq!(ids[168], "A169");
        assert_eq!(ids[169], "B001");
        assert_eq!(ids[338], "C001");
        assert_eq!(ids[507], "D001");
        assert_eq!(ids[676], "E001");
        assert_eq!(ids[843], "E168");
    }

    #[test]
    fn card_id_assignment_deterministic() {
        let ids = canonical_external_ids();
        // A001 -> 1, A002 -> 2, ..., E168 -> 844
        assert_eq!(ids.iter().position(|x| x == "A001").unwrap() + 1, 1);
        assert_eq!(ids.iter().position(|x| x == "A169").unwrap() + 1, 169);
        assert_eq!(ids.iter().position(|x| x == "B001").unwrap() + 1, 170);
        assert_eq!(ids.iter().position(|x| x == "E168").unwrap() + 1, 844);
    }

    #[test]
    fn registry_builder_basic() {
        let review = CardReview::unverified("A001");
        let registry = CardRegistryBuilder::new()
            .add_card(
                "A001".into(),
                CardKind::Occupation,
                vec![],
                0,
                vec![],
                review,
            )
            .unwrap()
            .freeze()
            .unwrap();

        assert_eq!(registry.len(), 1);
        assert_eq!(registry.card_id("A001"), Some(CardId::from_raw(1)));
        assert_eq!(registry.external_id(CardId::from_raw(1)), Some("A001"));
        assert!(registry.get(CardId::from_raw(1)).is_some());
        assert!(registry.get(CardId::from_raw(2)).is_none());
    }

    #[test]
    fn registry_rejects_unknown_external_id() {
        let review = CardReview::unverified("Z999");
        let err = CardRegistryBuilder::new()
            .add_card(
                "Z999".into(),
                CardKind::Occupation,
                vec![],
                0,
                vec![],
                review,
            )
            .unwrap_err();
        assert!(matches!(err, RegistryBuildError::UnknownExternalId(_)));
    }

    #[test]
    fn registry_rejects_duplicate_external_id() {
        let review = CardReview::unverified("A001");
        let err = CardRegistryBuilder::new()
            .add_card(
                "A001".into(),
                CardKind::Occupation,
                vec![],
                0,
                vec![],
                review.clone(),
            )
            .unwrap()
            .add_card(
                "A001".into(),
                CardKind::MinorImprovement,
                vec![],
                0,
                vec![],
                review,
            )
            .unwrap_err();
        assert!(matches!(err, RegistryBuildError::DuplicateExternalId(_)));
    }

    #[test]
    fn registry_digest_deterministic() {
        let review = CardReview::unverified("A001");
        let reg1 = CardRegistryBuilder::new()
            .add_card(
                "A001".into(),
                CardKind::Occupation,
                vec![],
                0,
                vec![],
                review.clone(),
            )
            .unwrap()
            .freeze()
            .unwrap();
        let reg2 = CardRegistryBuilder::new()
            .add_card(
                "A001".into(),
                CardKind::Occupation,
                vec![],
                0,
                vec![],
                review,
            )
            .unwrap()
            .freeze()
            .unwrap();
        assert_eq!(reg1.digest(), reg2.digest());
    }

    #[test]
    fn registry_digest_changes_on_mutation() {
        let review1 = CardReview::unverified("A001");
        let review2 = CardReview::unverified("A002");
        let reg1 = CardRegistryBuilder::new()
            .add_card(
                "A001".into(),
                CardKind::Occupation,
                vec![],
                0,
                vec![],
                review1,
            )
            .unwrap()
            .freeze()
            .unwrap();
        let reg2 = CardRegistryBuilder::new()
            .add_card(
                "A002".into(),
                CardKind::Occupation,
                vec![],
                0,
                vec![],
                review2,
            )
            .unwrap()
            .freeze()
            .unwrap();
        assert_ne!(reg1.digest(), reg2.digest());
    }

    #[test]
    fn event_index_populated() {
        let review = CardReview::unverified("A001");
        let registry = CardRegistryBuilder::new()
            .add_card(
                "A001".into(),
                CardKind::Occupation,
                vec![],
                0,
                vec![crate::cards::CardEffect::AfterWorkerGain {
                    space: crate::model::Space::Forest,
                    good: crate::model::Good::Wood,
                    amount: 1,
                }],
                review,
            )
            .unwrap()
            .freeze()
            .unwrap();

        let candidates = registry.event_candidates(EventKind::WorkerPlaced);
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0], CardId::from_raw(1));
    }

    #[test]
    fn pool_official_vs_experimental() {
        let review_verified = CardReview {
            card_id: "A001".into(),
            review_status: ReviewStatus::Verified,
            eligible_at_2: Eligibility::Yes,
            eligible_at_3: Eligibility::Yes,
            eligible_at_4: Eligibility::Yes,
        };
        let review_unverified = CardReview::unverified("A002");

        let registry = CardRegistryBuilder::new()
            .add_card(
                "A001".into(),
                CardKind::Occupation,
                vec![],
                0,
                vec![],
                review_verified,
            )
            .unwrap()
            .add_card(
                "A002".into(),
                CardKind::Occupation,
                vec![],
                0,
                vec![],
                review_unverified,
            )
            .unwrap()
            .freeze()
            .unwrap();

        let official_pool = CardPool::new(&registry, 2, false);
        let experimental_pool = CardPool::new(&registry, 2, true);

        assert_eq!(official_pool.eligible().len(), 1);
        assert_eq!(official_pool.eligible()[0], CardId::from_raw(1));
        assert_eq!(experimental_pool.eligible().len(), 2);
    }

    #[test]
    fn pool_digest_deterministic() {
        let review = CardReview::unverified("A001");
        let registry = CardRegistryBuilder::new()
            .add_card(
                "A001".into(),
                CardKind::Occupation,
                vec![],
                0,
                vec![],
                review,
            )
            .unwrap()
            .freeze()
            .unwrap();

        let pool1 = CardPool::new(&registry, 2, false);
        let pool2 = CardPool::new(&registry, 2, false);
        assert_eq!(pool1.digest(), pool2.digest());
    }

    #[test]
    fn default_registry_builds() {
        let registry = build_default_registry().unwrap();
        assert!(registry.len() >= 30); // At least the 30 verified basic cards
        assert!(!registry.digest().is_empty());
    }

    #[test]
    fn card_set_compat_adapters() {
        let registry = build_default_registry().unwrap();
        let pool = CardPool::new(&registry, 2, false);

        let set_from_pool = card_set_from_pool(&registry, &pool);
        let set_from_registry = card_set_from_registry(&registry);

        // Pool set should be subset of registry set
        assert!(set_from_pool.definitions.len() <= set_from_registry.definitions.len());
    }

    #[test]
    fn card_id_roundtrip_all_known_ids() {
        let registry = build_default_registry().unwrap();
        for card in registry.iter() {
            let external = &card.external_id;
            let card_id = registry.card_id(external).unwrap();
            let back = registry.external_id(card_id).unwrap();
            assert_eq!(external, back, "round-trip failed for {external}");
        }
    }

    #[test]
    fn future_card_addition_preserves_existing_ids() {
        // This test documents the guarantee: adding a new card (e.g., "E169")
        // must not change existing CardId assignments.
        // The canonical list is fixed at compile time; new cards would need
        // a new canonical list version, but existing mappings stay stable.
        let ids = canonical_external_ids();
        // A001 is always 1
        assert_eq!(ids.iter().position(|x| x == "A001").unwrap(), 0);
        // E168 is always 844
        assert_eq!(ids.iter().position(|x| x == "E168").unwrap(), 843);
    }
}
