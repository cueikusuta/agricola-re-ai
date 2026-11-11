//! M10-CORE (#139): code-side official-review gate and player-count pools.
//!
//! This module does **not** implement card effects and does **not** claim that
//! any card has been checked against official material. It records, in a
//! machine-readable form, the review state and player eligibility that the
//! `#115` ledger tracks, and it exposes a fail-closed filter so that an
//! unverified card can never enter an official pool by accident.
//!
//! Design notes:
//! - Metadata lives beside `CardDefinition` instead of inside it, so the card
//!   effect engine and every existing `CardDefinition { .. }` literal stay
//!   untouched.
//! - Every implemented card defaults to `unverified`. Nothing is promoted to
//!   `verified` here; that requires an official source recorded by `#115`.
//! - Cards absent from the registry are treated as `unverified` (fail closed),
//!   never as usable.
use crate::cards::{
    verified_extra_simple_cards, verified_immediate_resource_cards, verified_minor_worker_cards,
    verified_multiplayer_passive_cards, verified_passive_cards, verified_round_and_harvest_cards,
    verified_scoring_cards, verified_simple_occupations, verified_worker_bonus_cards,
    CardDefinition, CardKind, CardSet,
};
use crate::registry::{CardPool, CardRegistry};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

/// Official cross-check state, mirroring the `#115` ledger values.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ReviewStatus {
    /// No official check has been performed.
    Unverified,
    /// Reviewed internally but still not confirmed against official material.
    ReviewedNeedsOfficial,
    /// Confirmed against an official source recorded in the ledger.
    Verified,
    /// Determined not to match the official card or otherwise unusable.
    Rejected,
}

impl ReviewStatus {
    /// Ledger spelling used by `docs/card-catalog-ae-audit.csv`.
    pub fn as_ledger_str(self) -> &'static str {
        match self {
            ReviewStatus::Unverified => "unverified",
            ReviewStatus::ReviewedNeedsOfficial => "reviewed-needs-official",
            ReviewStatus::Verified => "verified",
            ReviewStatus::Rejected => "rejected",
        }
    }

    pub fn from_ledger_str(value: &str) -> Option<Self> {
        match value {
            "unverified" => Some(ReviewStatus::Unverified),
            "reviewed-needs-official" => Some(ReviewStatus::ReviewedNeedsOfficial),
            "verified" => Some(ReviewStatus::Verified),
            "rejected" => Some(ReviewStatus::Rejected),
            _ => None,
        }
    }
}

/// Player-count eligibility, mirroring the `#115` ledger values.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Eligibility {
    Yes,
    No,
    Unverified,
}

impl Eligibility {
    pub fn as_ledger_str(self) -> &'static str {
        match self {
            Eligibility::Yes => "yes",
            Eligibility::No => "no",
            Eligibility::Unverified => "unverified",
        }
    }

    pub fn from_ledger_str(value: &str) -> Option<Self> {
        match value {
            "yes" => Some(Eligibility::Yes),
            "no" => Some(Eligibility::No),
            "unverified" => Some(Eligibility::Unverified),
            _ => None,
        }
    }
}

/// Review metadata for one card id. Effect data stays in `CardDefinition`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CardReview {
    pub card_id: String,
    pub review_status: ReviewStatus,
    pub eligible_at_2: Eligibility,
    pub eligible_at_3: Eligibility,
    pub eligible_at_4: Eligibility,
}

impl CardReview {
    /// Fail-closed default for a card that has not been officially checked.
    pub fn unverified(card_id: &str) -> Self {
        Self {
            card_id: card_id.to_owned(),
            review_status: ReviewStatus::Unverified,
            eligible_at_2: Eligibility::Unverified,
            eligible_at_3: Eligibility::Unverified,
            eligible_at_4: Eligibility::Unverified,
        }
    }

    /// `Some` only for supported player counts (2, 3, 4).
    pub fn eligibility_at(&self, players: usize) -> Option<Eligibility> {
        match players {
            2 => Some(self.eligible_at_2),
            3 => Some(self.eligible_at_3),
            4 => Some(self.eligible_at_4),
            _ => None,
        }
    }

    /// A card may be dealt only when it is officially verified *and* the
    /// requested player count is explicitly eligible.
    pub fn permits_use_at(&self, players: usize) -> bool {
        self.review_status == ReviewStatus::Verified
            && matches!(self.eligibility_at(players), Some(Eligibility::Yes))
    }
}

/// All card definitions the engine currently implements, deduplicated by id.
///
/// The union is derived from the same constructors used to build playable
/// decks, so it cannot silently drift from the options the engine can run.
pub fn builtin_implemented_cards() -> Vec<CardDefinition> {
    let mut cards = Vec::new();
    // Union over every supported player count so player-count-specific cards
    // are included as well (for example a two-player-only occupation).
    for players in 2..=4 {
        cards.extend(verified_scoring_cards(players));
        cards.extend(verified_minor_worker_cards(players));
        cards.extend(verified_worker_bonus_cards(players));
        cards.extend(verified_simple_occupations(players));
        cards.extend(verified_multiplayer_passive_cards(players));
    }
    cards.extend(verified_immediate_resource_cards());
    cards.extend(verified_round_and_harvest_cards());
    cards.extend(verified_passive_cards());
    cards.extend(verified_extra_simple_cards());
    let mut seen = BTreeSet::new();
    cards.retain(|card| seen.insert(card.id.clone()));
    cards
}

/// Build a registry that marks every supplied card `unverified`.
pub fn default_review_registry(cards: &[CardDefinition]) -> Vec<CardReview> {
    let mut seen = BTreeSet::new();
    cards
        .iter()
        .filter(|card| seen.insert(card.id.clone()))
        .map(|card| CardReview::unverified(&card.id))
        .collect()
}

/// Registry for the built-in card implementations. Currently every entry is
/// `unverified`; official verification is tracked by `#115` and is not
/// asserted by code.
pub fn builtin_review_registry() -> Vec<CardReview> {
    default_review_registry(&builtin_implemented_cards())
}

/// Cards that are `verified` and explicitly eligible for `players`.
///
/// Cards with no registry entry are excluded (fail closed). The returned
/// slice is empty while official verification is incomplete, which is the
/// correct and intended state.
pub fn verified_eligible_pool<'a>(
    set: &'a CardSet,
    registry: &[CardReview],
    players: usize,
) -> Vec<&'a CardDefinition> {
    set.definitions
        .iter()
        .filter(|definition| {
            // A malformed duplicated card definition must not inflate a G1/G2
            // count. Conflicting or duplicated reviews are ambiguous and must
            // never be resolved by whichever entry happens to come first.
            if set
                .definitions
                .iter()
                .filter(|card| card.id == definition.id)
                .count()
                != 1
            {
                return false;
            }
            let mut matches = registry
                .iter()
                .filter(|review| review.card_id == definition.id);
            match (matches.next(), matches.next()) {
                (Some(review), None) => review.permits_use_at(players),
                _ => false,
            }
        })
        .collect()
}

/// Count of officially verified occupations dealable at `players`.
pub fn usable_occupations(set: &CardSet, registry: &[CardReview], players: usize) -> usize {
    verified_eligible_pool(set, registry, players)
        .into_iter()
        .filter(|card| card.kind == CardKind::Occupation)
        .count()
}

/// Count of officially verified minor improvements dealable at `players`.
pub fn usable_minor_improvements(set: &CardSet, registry: &[CardReview], players: usize) -> usize {
    verified_eligible_pool(set, registry, players)
        .into_iter()
        .filter(|card| card.kind == CardKind::MinorImprovement)
        .count()
}

/// G1 is the M10 minimum (40+40); G2 is the practical target (80+80).
pub const G1_MIN_PER_KIND: usize = 40;
pub const G2_TARGET_PER_KIND: usize = 80;

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct PoolGateReport {
    pub players: usize,
    pub verified_occupations: usize,
    pub verified_minor_improvements: usize,
    pub g1_met: bool,
    pub g2_met: bool,
    pub registry_entries: usize,
    pub registry_verified: usize,
    pub registry_not_verified: usize,
}

/// Machine-readable G1/G2 summary for one player count.
/// LEGACY: Works with CardSet + Vec<CardReview>.
pub fn pool_gate_report(set: &CardSet, registry: &[CardReview], players: usize) -> PoolGateReport {
    let occupations = usable_occupations(set, registry, players);
    let minors = usable_minor_improvements(set, registry, players);
    let registry_verified = registry
        .iter()
        .filter(|review| review.review_status == ReviewStatus::Verified)
        .count();
    PoolGateReport {
        players,
        verified_occupations: occupations,
        verified_minor_improvements: minors,
        g1_met: occupations >= G1_MIN_PER_KIND && minors >= G1_MIN_PER_KIND,
        g2_met: occupations >= G2_TARGET_PER_KIND && minors >= G2_TARGET_PER_KIND,
        registry_entries: registry.len(),
        registry_verified,
        registry_not_verified: registry.len() - registry_verified,
    }
}

// ===== NEW API: CardRegistry + CardPool =====

/// Build a PoolGateReport from the new CardRegistry and CardPool (official mode).
pub fn pool_gate_report_new(registry: &CardRegistry, pool: &CardPool) -> PoolGateReport {
    let occupations = pool.occupation_count(registry);
    let minors = pool.minor_improvement_count(registry);
    let registry_verified = registry
        .iter()
        .filter(|c| c.review.review_status == ReviewStatus::Verified)
        .count();
    PoolGateReport {
        players: pool.players(),
        verified_occupations: occupations,
        verified_minor_improvements: minors,
        g1_met: occupations >= G1_MIN_PER_KIND && minors >= G1_MIN_PER_KIND,
        g2_met: occupations >= G2_TARGET_PER_KIND && minors >= G2_TARGET_PER_KIND,
        registry_entries: registry.len(),
        registry_verified,
        registry_not_verified: registry.len() - registry_verified,
    }
}

/// Get the official pool as CardIds (new API).
pub fn official_pool_card_ids(
    registry: &CardRegistry,
    players: usize,
) -> Vec<crate::registry::CardId> {
    let pool = CardPool::new(registry, players, false);
    pool.eligible().to_vec()
}

/// Get the experimental pool as CardIds (new API).
pub fn experimental_pool_card_ids(
    registry: &CardRegistry,
    players: usize,
) -> Vec<crate::registry::CardId> {
    let pool = CardPool::new(registry, players, true);
    pool.eligible().to_vec()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::CardKind;

    fn def(id: &str, kind: CardKind) -> CardDefinition {
        CardDefinition {
            use_limit: None,
            id: id.to_owned(),
            kind,
            cost: vec![],
            points: 0,
            effects: vec![],
        }
    }

    fn reviewed(
        id: &str,
        status: ReviewStatus,
        at_2: Eligibility,
        at_4: Eligibility,
    ) -> CardReview {
        CardReview {
            card_id: id.to_owned(),
            review_status: status,
            eligible_at_2: at_2,
            eligible_at_3: Eligibility::Unverified,
            eligible_at_4: at_4,
        }
    }

    #[test]
    fn default_registry_is_fail_closed_and_empty_pool() {
        let set = CardSet {
            definitions: vec![
                def("A001", CardKind::Occupation),
                def("B001", CardKind::MinorImprovement),
            ],
        };
        let registry = default_review_registry(&set.definitions);
        assert_eq!(registry.len(), 2);
        assert!(registry.iter().all(|r| !r.permits_use_at(4)));
        assert!(verified_eligible_pool(&set, &registry, 4).is_empty());
        assert_eq!(usable_occupations(&set, &registry, 4), 0);
        assert_eq!(usable_minor_improvements(&set, &registry, 4), 0);
    }

    #[test]
    fn only_verified_and_eligible_cards_enter_the_pool() {
        let set = CardSet {
            definitions: vec![
                def("A001", CardKind::Occupation),
                def("A002", CardKind::Occupation),
                def("A003", CardKind::Occupation),
                def("A004", CardKind::Occupation),
            ],
        };
        let registry = vec![
            reviewed(
                "A001",
                ReviewStatus::Verified,
                Eligibility::Yes,
                Eligibility::Yes,
            ),
            reviewed(
                "A002",
                ReviewStatus::Unverified,
                Eligibility::Yes,
                Eligibility::Yes,
            ),
            reviewed(
                "A003",
                ReviewStatus::ReviewedNeedsOfficial,
                Eligibility::Yes,
                Eligibility::Yes,
            ),
            reviewed(
                "A004",
                ReviewStatus::Verified,
                Eligibility::Yes,
                Eligibility::No,
            ),
        ];
        let pool = verified_eligible_pool(&set, &registry, 4);
        assert_eq!(pool.len(), 1);
        assert_eq!(pool[0].id, "A001");
        // A004 is verified but two-player-only, so it stays out of the 4p pool
        // while still being usable at two players.
        assert_eq!(usable_occupations(&set, &registry, 2), 2);
    }

    #[test]
    fn cards_missing_from_the_registry_are_excluded() {
        let set = CardSet {
            definitions: vec![def("A001", CardKind::MinorImprovement)],
        };
        let registry = vec![reviewed(
            "A999",
            ReviewStatus::Verified,
            Eligibility::Yes,
            Eligibility::Yes,
        )];
        assert!(verified_eligible_pool(&set, &registry, 4).is_empty());
    }

    #[test]
    fn duplicate_registry_entries_are_fail_closed() {
        let set = CardSet {
            definitions: vec![def("A001", CardKind::Occupation)],
        };
        let verified = reviewed(
            "A001",
            ReviewStatus::Verified,
            Eligibility::Yes,
            Eligibility::Yes,
        );
        let unverified = reviewed(
            "A001",
            ReviewStatus::Unverified,
            Eligibility::Unverified,
            Eligibility::Unverified,
        );
        // Order must not determine whether an untrusted duplicate is dealt.
        for registry in [
            vec![verified.clone(), unverified.clone()],
            vec![unverified.clone(), verified.clone()],
            vec![verified.clone(), verified.clone()],
        ] {
            assert!(verified_eligible_pool(&set, &registry, 4).is_empty());
            assert_eq!(usable_occupations(&set, &registry, 4), 0);
        }
    }

    #[test]
    fn duplicate_definitions_do_not_inflate_g1_g2_gate_counts() {
        let set = CardSet {
            definitions: vec![
                def("A001", CardKind::Occupation),
                def("A001", CardKind::Occupation),
            ],
        };
        let registry = vec![reviewed(
            "A001",
            ReviewStatus::Verified,
            Eligibility::Yes,
            Eligibility::Yes,
        )];
        assert!(verified_eligible_pool(&set, &registry, 4).is_empty());
    }

    #[test]
    fn player_counts_are_kept_separate() {
        let set = CardSet {
            definitions: vec![
                def("A001", CardKind::Occupation),
                def("A002", CardKind::Occupation),
            ],
        };
        let registry = vec![
            reviewed(
                "A001",
                ReviewStatus::Verified,
                Eligibility::Yes,
                Eligibility::No,
            ),
            reviewed(
                "A002",
                ReviewStatus::Verified,
                Eligibility::No,
                Eligibility::Yes,
            ),
        ];
        assert_eq!(usable_occupations(&set, &registry, 2), 1);
        assert_eq!(usable_occupations(&set, &registry, 4), 1);
        assert_eq!(usable_occupations(&set, &registry, 3), 0);
    }

    #[test]
    fn unsupported_player_counts_are_never_eligible() {
        let review = reviewed(
            "A001",
            ReviewStatus::Verified,
            Eligibility::Yes,
            Eligibility::Yes,
        );
        assert_eq!(review.eligibility_at(5), None);
        assert!(!review.permits_use_at(5));
    }

    #[test]
    fn ledger_spellings_round_trip() {
        for status in [
            ReviewStatus::Unverified,
            ReviewStatus::ReviewedNeedsOfficial,
            ReviewStatus::Verified,
            ReviewStatus::Rejected,
        ] {
            assert_eq!(
                ReviewStatus::from_ledger_str(status.as_ledger_str()),
                Some(status)
            );
        }
        for eligibility in [Eligibility::Yes, Eligibility::No, Eligibility::Unverified] {
            assert_eq!(
                Eligibility::from_ledger_str(eligibility.as_ledger_str()),
                Some(eligibility)
            );
        }
        assert_eq!(ReviewStatus::from_ledger_str("bogus"), None);
        assert_eq!(Eligibility::from_ledger_str("maybe"), None);
    }

    #[test]
    fn builtin_pool_is_honestly_empty_and_gates_unmet() {
        let cards = builtin_implemented_cards();
        assert!(cards.len() >= 30, "the built-in pool must stay populated");
        let set = CardSet {
            definitions: cards.clone(),
        };
        let registry = builtin_review_registry();
        assert_eq!(registry.len(), cards.len());
        for players in [2, 3, 4] {
            let report = pool_gate_report(&set, &registry, players);
            assert_eq!(report.verified_occupations, 0, "players={players}");
            assert_eq!(report.verified_minor_improvements, 0, "players={players}");
            assert!(!report.g1_met, "players={players}");
            assert!(!report.g2_met, "players={players}");
        }
    }

    #[test]
    fn g1_and_g2_gates_are_independent() {
        let mut definitions = Vec::new();
        let mut registry = Vec::new();
        for index in 0..G2_TARGET_PER_KIND {
            let id = format!("O{index:03}");
            definitions.push(def(&id, CardKind::Occupation));
            registry.push(CardReview {
                card_id: id,
                review_status: ReviewStatus::Verified,
                eligible_at_2: Eligibility::Yes,
                eligible_at_3: Eligibility::Yes,
                eligible_at_4: Eligibility::Yes,
            });
        }
        // 39 minors is still below G1 (40), so both gates stay false.
        for index in 0..(G1_MIN_PER_KIND - 1) {
            let id = format!("M{index:03}");
            definitions.push(def(&id, CardKind::MinorImprovement));
            registry.push(CardReview {
                card_id: id,
                review_status: ReviewStatus::Verified,
                eligible_at_2: Eligibility::Yes,
                eligible_at_3: Eligibility::Yes,
                eligible_at_4: Eligibility::Yes,
            });
        }
        let set = CardSet { definitions };
        let report = pool_gate_report(&set, &registry, 4);
        assert_eq!(report.verified_occupations, G2_TARGET_PER_KIND);
        assert_eq!(report.verified_minor_improvements, G1_MIN_PER_KIND - 1);
        assert!(!report.g1_met);
        assert!(!report.g2_met);
    }

    // ===== NEW API TESTS =====

    #[test]
    fn new_api_pool_gate_report_matches_legacy() {
        let registry = crate::registry::build_default_registry().unwrap();
        let pool = crate::registry::CardPool::new(&registry, 2, false);
        let report_new = pool_gate_report_new(&registry, &pool);

        // Also test with legacy
        let set = registry.to_card_set();
        let legacy_registry = builtin_review_registry();
        let report_legacy = pool_gate_report(&set, &legacy_registry, 2);

        // Both should show 0 verified since all are unverified
        assert_eq!(
            report_new.verified_occupations,
            report_legacy.verified_occupations
        );
        assert_eq!(
            report_new.verified_minor_improvements,
            report_legacy.verified_minor_improvements
        );
    }

    #[test]
    fn official_pool_card_ids_subset_of_experimental() {
        let registry = crate::registry::build_default_registry().unwrap();
        let official = official_pool_card_ids(&registry, 2);
        let experimental = experimental_pool_card_ids(&registry, 2);
        for id in &official {
            assert!(
                experimental.contains(id),
                "official id {id:?} missing from experimental"
            );
        }
    }
}
