//! Review O1 Stage 1: spec JSON <-> Rust implementation reconciliation tests.
//!
//! Four gap-detection checks (a)-(d) plus an explicit allowance list of the
//! breaks that already exist at the start of this work. Any break NOT in the
//! allowance list fails; any allowance entry matching no real break fails.
//! No wildcards / prefixes: every entry is an exact card ID.

use agricola_re_ai::card_pool::Eligibility;
use agricola_re_ai::card_spec::{load_card_specs, CardSpec, SpecCost, SpecElig};
use agricola_re_ai::cards::{card_set_for_game, default_registry};
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

fn specs_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("data")
        .join("bga-card-specs")
}

fn ledger_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("data")
        .join("bga-card-base-ledger.csv")
}

#[derive(Clone, Debug)]
#[allow(dead_code)]
struct Break {
    test: char,
    card_id: String,
    player: Option<u8>,
    detail: String,
}

#[derive(Clone, Copy, Debug)]
struct AllowEntry {
    test: char,
    card_id: &'static str,
    player: Option<u8>,
    reason: &'static str,
}

// Existing breaks at the start of this work. Exact card IDs only, no
// wildcards/prefixes. Every entry must match a real break (provision (ii)),
// and every real break must be listed (provision (i)).
const ALLOWANCE: &[AllowEntry] = &[
    AllowEntry {
        test: 'a',
        card_id: "C076",
        player: None,
        reason: "registered in default_registry() but data/bga-card-specs/C076.json does not exist",
    },
    AllowEntry {
        test: 'a',
        card_id: "C106",
        player: None,
        reason: "registered in default_registry() but data/bga-card-specs/C106.json does not exist",
    },
    AllowEntry {
        test: 'a',
        card_id: "C111",
        player: None,
        reason: "registered in default_registry() but data/bga-card-specs/C111.json does not exist",
    },
    AllowEntry {
        test: 'a',
        card_id: "C116",
        player: None,
        reason: "registered in default_registry() but data/bga-card-specs/C116.json does not exist",
    },
    AllowEntry {
        test: 'a',
        card_id: "E107",
        player: None,
        reason: "registered in default_registry() but data/bga-card-specs/E107.json does not exist",
    },
    AllowEntry {
        test: 'a',
        card_id: "E111",
        player: None,
        reason: "registered in default_registry() but data/bga-card-specs/E111.json does not exist",
    },
    AllowEntry {
        test: 'a',
        card_id: "E113",
        player: None,
        reason: "registered in default_registry() but data/bga-card-specs/E113.json does not exist",
    },
    AllowEntry {
        test: 'a',
        card_id: "E117",
        player: None,
        reason: "registered in default_registry() but data/bga-card-specs/E117.json does not exist",
    },
    AllowEntry {
        test: 'a',
        card_id: "E118",
        player: None,
        reason: "registered in default_registry() but data/bga-card-specs/E118.json does not exist",
    },
    AllowEntry {
        test: 'c',
        card_id: "B102",
        player: Some(4),
        reason: "spec eligibility 4=yes but Rust eligibility_at(4)=No: B102's 4-player effect is not implemented (merge_card_variants marks 4p ineligible); B121/B102 unification is P1-2 scope, left untouched",
    },
    AllowEntry {
        test: 'c',
        card_id: "C141",
        player: Some(2),
        reason: "spec eligibility 2=no but Rust eligibility_at(2)=Yes: C141 has no MinimumPlayers guard so Rust treats it as 2p-eligible; existing spec-vs-Rust gap, left as allowance entry",
    },
    AllowEntry {
        test: 'c',
        card_id: "C147",
        player: Some(2),
        reason: "spec eligibility 2=no but Rust eligibility_at(2)=Yes: C147 has no MinimumPlayers guard so Rust treats it as 2p-eligible; existing spec-vs-Rust gap, left as allowance entry",
    },
    AllowEntry {
        test: 'd',
        card_id: "C141",
        player: Some(2),
        reason: "spec eligibility 2=no but C141 present in experimental pool at 2 players; pool-membership consequence of the C141 spec-vs-Rust eligibility gap (also listed under test c)",
    },
    AllowEntry {
        test: 'd',
        card_id: "C147",
        player: Some(2),
        reason: "spec eligibility 2=no but C147 present in experimental pool at 2 players; pool-membership consequence of the C147 spec-vs-Rust eligibility gap (also listed under test c)",
    },
];

fn load_specs() -> Vec<CardSpec> {
    load_card_specs(&specs_dir()).expect("spec JSON directory must load without parse errors")
}

fn load_ledger_ids() -> BTreeSet<String> {
    let text = std::fs::read_to_string(ledger_path()).expect("ledger must be readable");
    let text = text.trim_start_matches('\u{FEFF}');
    let mut ids = BTreeSet::new();
    for (index, line) in text.lines().enumerate() {
        if index == 0 || line.trim().is_empty() {
            continue;
        }
        let fields: Vec<&str> = line.split(',').collect();
        if fields.is_empty() || fields[0].is_empty() {
            continue;
        }
        if fields.len() < 28 {
            panic!("ledger row {index} has fewer than 28 fields");
        }
        ids.insert(fields[0].to_owned());
    }
    ids
}

fn check(test: char, breaks: &[Break]) {
    let mut problems: Vec<String> = Vec::new();
    for b in breaks {
        let allowlisted = ALLOWANCE
            .iter()
            .any(|a| a.test == test && a.card_id == b.card_id && a.player == b.player);
        if !allowlisted {
            problems.push(format!(
                "  unallowlisted break test={test} card={} player={:?}: {}",
                b.card_id, b.player, b.detail
            ));
        }
    }
    for a in ALLOWANCE.iter().filter(|a| a.test == test) {
        if a.reason.is_empty() {
            problems.push(format!(
                "  allowance entry test={test} card={} player={:?} has empty reason",
                a.card_id, a.player
            ));
        } else if !breaks
            .iter()
            .any(|b| b.card_id == a.card_id && b.player == a.player)
        {
            problems.push(format!(
                "  allowance entry test={test} card={} player={:?} matches no break: {}",
                a.card_id, a.player, a.reason
            ));
        }
    }
    if !problems.is_empty() {
        panic!(
            "test {test} allowance/breaks mismatch:\n{}",
            problems.join("\n")
        );
    }
}

#[test]
fn specs_load_all_and_unique() {
    let specs = load_specs();
    assert!(!specs.is_empty(), "specs must be non-empty");
    let ids: BTreeSet<&str> = specs.iter().map(|s| s.card_id.as_str()).collect();
    assert_eq!(ids.len(), specs.len(), "spec card_id must be unique");
}

#[test]
fn t_a_id_sets() {
    let specs = load_specs();
    let spec_ids: BTreeSet<&str> = specs.iter().map(|s| s.card_id.as_str()).collect();
    let reg_ids: BTreeSet<String> = default_registry()
        .iter()
        .map(|rc| rc.external_id.clone())
        .collect();
    let ledger_ids = load_ledger_ids();

    let mut breaks: Vec<Break> = Vec::new();
    // A1: registered in code but no spec JSON at all.
    for id in &reg_ids {
        if !spec_ids.contains(id.as_str()) {
            breaks.push(Break {
                test: 'a',
                card_id: id.clone(),
                player: None,
                detail: "registered in code but no spec JSON".to_owned(),
            });
        }
    }
    // A2: spec JSON with no base ledger row.
    for card_id in &spec_ids {
        if !ledger_ids.contains(*card_id) {
            breaks.push(Break {
                test: 'a',
                card_id: card_id.to_string(),
                player: None,
                detail: "spec JSON has no base ledger row".to_owned(),
            });
        }
    }
    check('a', &breaks);
}

#[test]
fn t_b_implemented_is_registered() {
    let specs = load_specs();
    let reg_ids: BTreeSet<String> = default_registry()
        .iter()
        .map(|rc| rc.external_id.clone())
        .collect();

    let mut breaks: Vec<Break> = Vec::new();
    // spec implementation_status == implemented (or tested) => must be registered.
    for s in &specs {
        if (s.implementation_status == "implemented" || s.implementation_status == "tested")
            && !reg_ids.contains(&s.card_id)
        {
            breaks.push(Break {
                test: 'b',
                card_id: s.card_id.clone(),
                player: None,
                detail: format!(
                    "spec implementation_status={} but not registered in code",
                    s.implementation_status
                ),
            });
        }
    }
    check('b', &breaks);
}

#[test]
fn t_c_eligibility_vs_minimum_players() {
    let specs = load_specs();
    let spec_map: BTreeMap<&str, &CardSpec> =
        specs.iter().map(|s| (s.card_id.as_str(), s)).collect();
    let registry = default_registry();

    let mut breaks: Vec<Break> = Vec::new();
    // Rust-side eligibility is `CardReview::eligibility_at`, which is compiled
    // from the card's `MinimumPlayers`/`MaximumPlayers` prerequisites by
    // `registry::merge_card_variants`.
    for rc in registry.iter() {
        let Some(spec) = spec_map.get(rc.external_id.as_str()) else {
            continue;
        };
        for p in [2u8, 3, 4] {
            let se = spec.eligible_at(p.into()).expect("2/3/4");
            let re = rc.review.eligibility_at(p.into()).expect("2/3/4");
            match (se, re) {
                (SpecElig::No, Eligibility::Yes) => {
                    breaks.push(Break {
                        test: 'c',
                        card_id: rc.external_id.clone(),
                        player: Some(p),
                        detail: format!("spec eligibility {p}=no but Rust eligibility_at says yes"),
                    });
                }
                (SpecElig::Yes, Eligibility::No) => {
                    breaks.push(Break {
                        test: 'c',
                        card_id: rc.external_id.clone(),
                        player: Some(p),
                        detail: format!("spec eligibility {p}=yes but Rust eligibility_at says no"),
                    });
                }
                _ => {}
            }
        }
    }
    check('c', &breaks);
}

#[test]
fn t_d_experimental_pool_no_ineligible() {
    let specs = load_specs();
    let spec_map: BTreeMap<&str, &CardSpec> =
        specs.iter().map(|s| (s.card_id.as_str(), s)).collect();

    let mut breaks: Vec<Break> = Vec::new();
    for p in [2u8, 3, 4] {
        let pool = card_set_for_game(p.into(), true);
        for def in &pool.definitions {
            let Some(spec) = spec_map.get(def.id.as_str()) else {
                continue;
            };
            if spec.eligible_at(p.into()) == Some(SpecElig::No) {
                breaks.push(Break {
                    test: 'd',
                    card_id: def.id.clone(),
                    player: Some(p),
                    detail: format!(
                        "spec eligibility {p}=no but card present in experimental pool"
                    ),
                });
            }
        }
    }
    check('d', &breaks);
}

#[test]
fn cost_distinguishes_null_from_empty() {
    let specs = load_specs();
    let mut null_seen = false;
    let mut free_seen = false;
    for s in &specs {
        match s.cost {
            SpecCost::Unconfirmed => null_seen = true,
            SpecCost::Free => free_seen = true,
            SpecCost::Items(_) => {}
        }
    }
    assert!(
        null_seen,
        "expect at least one spec with cost == null (unconfirmed)"
    );
    assert!(free_seen, "expect at least one spec with cost == [] (free)");
}
