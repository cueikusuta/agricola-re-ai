//! M10-CORE (#139): machine-readable verified-pool / G1-G2 report.
//!
//! Output is one JSON object. `verified_*` counts are `0` until `#115` records
//! official verification; that is a correct report, not a failure.
use agricola_re_ai::card_pool::{
    builtin_implemented_cards, builtin_review_registry, pool_gate_report, ReviewStatus,
};
use agricola_re_ai::cards::CardSet;
use serde_json::json;

fn main() {
    let definitions = builtin_implemented_cards();
    let set = CardSet { definitions };
    let registry = builtin_review_registry();

    let per_players = [2usize, 3, 4]
        .iter()
        .map(|&players| serde_json::to_value(pool_gate_report(&set, &registry, players)).unwrap())
        .collect::<Vec<_>>();

    let by_status = |status: ReviewStatus| {
        registry
            .iter()
            .filter(|entry| entry.review_status == status)
            .count()
    };

    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "schema_version": 1,
            "source": "src/card_pool.rs",
            "implemented_card_count": set.definitions.len(),
            "registry_entry_count": registry.len(),
            "registry_status_counts": {
                "unverified": by_status(ReviewStatus::Unverified),
                "reviewed-needs-official": by_status(ReviewStatus::ReviewedNeedsOfficial),
                "verified": by_status(ReviewStatus::Verified),
                "rejected": by_status(ReviewStatus::Rejected),
            },
            "gates": {
                "g1_min_per_kind": 40,
                "g2_target_per_kind": 80,
            },
            "official_verification_included": false,
            "note": "No card is counted as official until #115 records an official source; zero is the expected current result.",
            "per_players": per_players,
        }))
        .expect("pool report JSON")
    );
}
