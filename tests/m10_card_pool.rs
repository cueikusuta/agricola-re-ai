//! M10-CORE (#139): the code-side card definitions and the `#115` ledger must
//! agree on ids and normalized card kinds. This is a structural cross-check,
//! not a claim that any card has been officially verified.
use agricola_re_ai::card_pool::{
    builtin_implemented_cards, builtin_review_registry, default_review_registry, pool_gate_report,
    Eligibility, ReviewStatus,
};
use agricola_re_ai::cards::{CardKind, CardSet};
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

fn ledger_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("docs")
        .join(name)
}

/// Minimal parser: the ledger quotes every field and none of the current
/// values contain embedded commas or quotes.
fn ledger_types(name: &str) -> BTreeMap<String, String> {
    let text = std::fs::read_to_string(ledger_path(name)).expect("ledger must be readable");
    let mut rows = BTreeMap::new();
    for (index, line) in text.lines().enumerate() {
        if index == 0 || line.trim().is_empty() {
            continue;
        }
        let mut fields = line.split(',').map(|field| field.trim_matches('"'));
        let card_id = fields.next().unwrap_or_default().to_owned();
        let card_type = fields.next().unwrap_or_default().to_owned();
        rows.insert(card_id, card_type);
    }
    rows
}

fn code_kind(card: &agricola_re_ai::cards::CardDefinition) -> &'static str {
    match card.kind {
        CardKind::Occupation => "occupation",
        CardKind::MinorImprovement => "minor_improvement",
    }
}

#[test]
fn implemented_ids_are_unique_and_match_the_audit_ledger() {
    let cards = builtin_implemented_cards();
    let ids = cards
        .iter()
        .map(|card| card.id.as_str())
        .collect::<Vec<_>>();
    let unique = ids.iter().copied().collect::<BTreeSet<_>>();
    assert_eq!(unique.len(), ids.len(), "duplicate implemented card id");

    let ledger = ledger_types("card-catalog-ae-audit.csv");
    assert_eq!(ledger.len(), 840, "audit ledger must hold all 840 rows");
    let missing = ids
        .iter()
        .filter(|id| !ledger.contains_key(**id))
        .copied()
        .collect::<Vec<_>>();
    assert!(
        missing.is_empty(),
        "implemented ids absent from ledger: {missing:?}"
    );
}

#[test]
fn normalized_kind_agrees_with_code_for_implemented_cards() {
    let ledger = ledger_types("card-catalog-ae-audit.csv");
    let mismatches = builtin_implemented_cards()
        .into_iter()
        .filter_map(|card| {
            let ledger_type = ledger.get(&card.id)?;
            (ledger_type != code_kind(&card)).then(|| {
                format!(
                    "{}: code={} ledger={ledger_type}",
                    card.id,
                    code_kind(&card)
                )
            })
        })
        .collect::<Vec<_>>();
    assert!(
        mismatches.is_empty(),
        "card kind differs between code and ledger: {mismatches:?}"
    );
}

#[test]
fn registry_covers_every_implemented_card_exactly_once() {
    let cards = builtin_implemented_cards();
    let registry = builtin_review_registry();
    let ids = cards
        .iter()
        .map(|card| card.id.as_str())
        .collect::<BTreeSet<_>>();
    let registry_ids = registry
        .iter()
        .map(|entry| entry.card_id.as_str())
        .collect::<BTreeSet<_>>();
    assert_eq!(ids, registry_ids);
    assert_eq!(registry.len(), ids.len());
    assert!(registry
        .iter()
        .all(|entry| entry.review_status == ReviewStatus::Unverified));
}

#[test]
fn verified_pool_stays_empty_until_official_review_completes() {
    let set = CardSet {
        definitions: builtin_implemented_cards(),
    };
    let registry = builtin_review_registry();
    for players in [2usize, 3, 4] {
        let report = pool_gate_report(&set, &registry, players);
        assert_eq!(report.verified_occupations, 0, "players={players}");
        assert_eq!(report.verified_minor_improvements, 0, "players={players}");
        assert!(!report.g1_met);
        assert!(!report.g2_met);
        assert_eq!(report.registry_verified, 0);
    }
}

#[test]
fn default_registry_never_promotes_a_card() {
    let cards = builtin_implemented_cards();
    let registry = default_review_registry(&cards);
    assert!(registry
        .iter()
        .all(|entry| !entry.permits_use_at(2) && !entry.permits_use_at(4)));
    // Sanity: eligibility values never leak as `Yes` by default.
    assert!(registry
        .iter()
        .all(|entry| entry.eligible_at_2 != Eligibility::Yes));
}
