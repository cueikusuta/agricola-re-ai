//! Tests for ch02 B-deck on-play simple cards: B002, B003, B005, B008, B009
//!
//! These tests verify:
//! - Registry/construction path reachability
//! - Effect-only play via `cards::apply_on_play_effects` (supply delta)
//! - Dispatcher route via `card_events::dispatch_play` (hand-dependent effects)
//! - Legal-play / prerequisite gating via `cards::play`/`cards::can_play`

use agricola_re_ai::card_events::dispatch_play;
use agricola_re_ai::cards::{
    apply_on_play_effects, CardDefinition, CardEffect, CardKind, CardSet, PostPlayDestination,
};
use agricola_re_ai::model::{neighbors, Animal, Game, Good, Pasture, Player, FARM_CELLS};
use agricola_re_ai::registry::build_default_registry;

fn new_set(defs: Vec<CardDefinition>) -> CardSet {
    CardSet { definitions: defs }
}

fn occ_def(id: &str) -> CardDefinition {
    CardDefinition {
        use_limit: None,
        id: id.to_owned(),
        kind: CardKind::Occupation,
        cost: vec![],
        points: 0,
        effects: vec![],
        post_play: PostPlayDestination::PlayedArea,
    }
}

#[allow(dead_code)]
fn minor_def(id: &str) -> CardDefinition {
    CardDefinition {
        use_limit: None,
        id: id.to_owned(),
        kind: CardKind::MinorImprovement,
        cost: vec![],
        points: 0,
        effects: vec![],
        post_play: PostPlayDestination::PlayedArea,
    }
}

/// Dispatcher route: pay the printed cost manually (dispatch_play does not
/// validate cost), then run `dispatch_play` for card `id`.
/// Returns both hands so callers can assert left-neighbor reachability
/// for pass-left cards (P2-3).
fn dispatch_card(
    set: &CardSet,
    player: &mut Player,
    hand: &mut [String],
    id: &str,
) -> (Vec<String>, Vec<String>) {
    let card = set
        .definition(id)
        .unwrap_or_else(|| panic!("{id} missing from set"));
    for (good, amount) in &card.cost {
        player
            .supply
            .pay(*good, *amount)
            .unwrap_or_else(|e| panic!("pay {id} cost: {e}"));
    }
    let mut cards = agricola_re_ai::cards::CardState {
        hands: vec![hand.to_vec(), vec![]],
        ..Default::default()
    };
    let mut players = vec![player.clone(), Player::new(1, 0)];
    // Caller-side zone move: the new card is already in played_cards when
    // dispatch_play runs (see card_events dispatch_play Phase 3 comment).
    players[0].played_cards.push(id.to_owned());
    let mut market = vec![];
    let mut well = std::collections::BTreeMap::new();
    let mut budget = 64u32;
    let mut uses = std::collections::BTreeMap::new();
    let limits = std::collections::BTreeMap::new();
    let mut pending = None;
    dispatch_play(
        set,
        0,
        id,
        0,
        &mut cards,
        &mut players,
        &mut market,
        &mut well,
        1,
        &std::collections::BTreeSet::new(),
        &mut budget,
        &mut uses,
        &limits,
        &mut pending,
    )
    .unwrap_or_else(|e| panic!("dispatch_play {id}: {e}"));
    *player = players[0].clone();
    (cards.hands[0].clone(), cards.hands[1].clone())
}

// ---------------- Registry reachability ----------------

#[test]
fn ch02_cards_reachable_via_default_registry() {
    let registry = build_default_registry().expect("default registry builds");
    for id in ["B002", "B003", "B005", "B008", "B009"] {
        let card_id = registry
            .card_id(id)
            .unwrap_or_else(|| panic!("{id} must have a CardId"));
        let card = registry
            .get(card_id)
            .unwrap_or_else(|| panic!("{id} must be retrievable"));
        assert_eq!(card.external_id, id, "round-trip external_id for {id}");
        assert_eq!(
            card.kind,
            CardKind::MinorImprovement,
            "{id} must be MinorImprovement"
        );
        assert!(
            card.post_play == PostPlayDestination::LeftNeighborHand,
            "{id} must pass left"
        );
    }
}

// ---------------- B002 小牧場 (Mini Pasture) ----------------

#[test]
fn b002_normal_grants_free_pasture() {
    let set = new_set(vec![CardDefinition {
        use_limit: None,
        id: "B002".to_owned(),
        kind: CardKind::MinorImprovement,
        cost: vec![(Good::Food, 2)],
        points: 0,
        effects: vec![CardEffect::OnPlayBuildPasture { spaces: 1 }],
        post_play: PostPlayDestination::LeftNeighborHand,
    }]);
    let mut player = Player::new(0, 0);
    player.supply.food = 2;
    apply_on_play_effects(&set, &mut player, "B002").expect("apply B002");
    // Player has no pasture initially, so any empty cell is valid
    assert_eq!(player.pastures.len(), 1, "B002 must create 1 pasture");
    assert_eq!(player.pastures[0].cells.len(), 1, "pasture must be 1 cell");
}

#[test]
fn b002_adjacent_to_existing_pasture() {
    let set = new_set(vec![CardDefinition {
        use_limit: None,
        id: "B002".to_owned(),
        kind: CardKind::MinorImprovement,
        cost: vec![(Good::Food, 2)],
        points: 0,
        effects: vec![CardEffect::OnPlayBuildPasture { spaces: 1 }],
        post_play: PostPlayDestination::LeftNeighborHand,
    }]);
    let mut player = Player::new(0, 0);
    player.supply.food = 2;
    // Manually create a pasture at cell 0 (top-left)
    player.farm[0].tile = agricola_re_ai::model::Tile::Pasture { id: 1 };
    player.pastures.push(Pasture {
        id: 1,
        cells: vec![0],
        animal: None,
        quantity: 0,
    });
    apply_on_play_effects(&set, &mut player, "B002").expect("apply B002");
    assert_eq!(player.pastures.len(), 2, "B002 must create another pasture");
    // The new pasture should be adjacent to the existing one
    let new_pasture_cell = player.pastures[1].cells[0];
    assert!(neighbors(new_pasture_cell).contains(&0));
}

#[test]
fn b002_no_pasture_when_no_empty_cells() {
    let set = new_set(vec![CardDefinition {
        use_limit: None,
        id: "B002".to_owned(),
        kind: CardKind::MinorImprovement,
        cost: vec![(Good::Food, 2)],
        points: 0,
        effects: vec![CardEffect::OnPlayBuildPasture { spaces: 1 }],
        post_play: PostPlayDestination::LeftNeighborHand,
    }]);
    let mut player = Player::new(0, 0);
    player.supply.food = 2;
    // Fill all farm cells
    for i in 0..FARM_CELLS {
        player.farm[i].tile = agricola_re_ai::model::Tile::Field {
            crop: Some(agricola_re_ai::model::Crop::Grain),
            remaining: 1,
        };
    }
    apply_on_play_effects(&set, &mut player, "B002").expect("apply B002");
    assert_eq!(
        player.pastures.len(),
        0,
        "no empty cells -> no pasture created"
    );
}

// ---------------- B003 密造酒 (Moonshine) ----------------

#[test]
fn b003_normal_pay_2_food_play_occupation() {
    let set = new_set(vec![
        CardDefinition {
            use_limit: None,
            id: "B003".to_owned(),
            kind: CardKind::MinorImprovement,
            cost: vec![],
            points: 0,
            effects: vec![CardEffect::OnPlayDrawPlayOrPassOccupation { food_cost: 2 }],
            post_play: PostPlayDestination::LeftNeighborHand,
        },
        occ_def("test_occ"),
    ]);
    let mut player = Player::new(0, 0);
    player.supply.food = 2;
    let mut hand = vec!["B003".to_owned(), "test_occ".to_owned()];
    let _ = dispatch_card(&set, &mut player, &mut hand, "B003");
    // Occupation should be played (moved from hand to played_cards)
    assert!(player.played_cards.contains(&"test_occ".to_string()));
    assert_eq!(player.supply.food, 0, "2 food paid for occupation");
}

#[test]
fn b003_pass_when_no_food() {
    let set = new_set(vec![
        CardDefinition {
            use_limit: None,
            id: "B003".to_owned(),
            kind: CardKind::MinorImprovement,
            cost: vec![],
            points: 0,
            effects: vec![CardEffect::OnPlayDrawPlayOrPassOccupation { food_cost: 2 }],
            post_play: PostPlayDestination::LeftNeighborHand,
        },
        occ_def("test_occ"),
    ]);
    let mut player = Player::new(0, 0);
    player.supply.food = 0;
    let mut hand = vec!["B003".to_owned(), "test_occ".to_owned()];
    let (hand, left) = dispatch_card(&set, &mut player, &mut hand, "B003");
    // Occupation should be passed to left neighbour (not in player's hand or played)
    assert!(!hand.contains(&"test_occ".to_string()));
    assert!(!player.played_cards.contains(&"test_occ".to_string()));
    // Reachability: the drawn occupation must actually reach the left
    // neighbour's hand (P2-3), not merely vanish from this player.
    assert!(
        left.contains(&"test_occ".to_string()),
        "drawn occupation must reach the left neighbour's hand"
    );
}

#[test]
fn b003_no_occupation_in_hand_does_nothing() {
    let set = new_set(vec![CardDefinition {
        use_limit: None,
        id: "B003".to_owned(),
        kind: CardKind::MinorImprovement,
        cost: vec![],
        points: 0,
        effects: vec![CardEffect::OnPlayDrawPlayOrPassOccupation { food_cost: 2 }],
        post_play: PostPlayDestination::LeftNeighborHand,
    }]);
    let mut player = Player::new(0, 0);
    player.supply.food = 2;
    let mut hand = vec!["B003".to_owned()];
    let _ = dispatch_card(&set, &mut player, &mut hand, "B003");
    // No occupation to draw, nothing happens
    assert_eq!(player.supply.food, 2);
}

// ---------------- B005 経験の蓄積 (Store of Experience) ----------------

#[test]
fn b005_normal_0_to_4_occupations_grants_stone() {
    let set = new_set(vec![CardDefinition {
        use_limit: None,
        id: "B005".to_owned(),
        kind: CardKind::MinorImprovement,
        cost: vec![],
        points: 0,
        effects: vec![CardEffect::OnPlayGainByHandOccupationCount {
            bands: vec![
                (4, Good::Stone),
                (5, Good::Reed),
                (6, Good::Clay),
                (7, Good::Wood),
            ],
        }],
        post_play: PostPlayDestination::LeftNeighborHand,
    }]);
    let mut player = Player::new(0, 0);
    let mut hand = vec!["B005".to_owned(), "occ1".to_owned(), "occ2".to_owned()];
    // Need to add occupations to the set for hand detection
    let set = CardSet {
        definitions: vec![set.definitions[0].clone(), occ_def("occ1"), occ_def("occ2")],
    };
    let _ = dispatch_card(&set, &mut player, &mut hand, "B005");
    assert_eq!(player.supply.stone, 1, "2 occupations -> stone");
}

#[test]
fn b005_boundary_4_occupations_grants_stone() {
    let set = new_set(vec![
        CardDefinition {
            use_limit: None,
            id: "B005".to_owned(),
            kind: CardKind::MinorImprovement,
            cost: vec![],
            points: 0,
            effects: vec![CardEffect::OnPlayGainByHandOccupationCount {
                bands: vec![
                    (4, Good::Stone),
                    (5, Good::Reed),
                    (6, Good::Clay),
                    (7, Good::Wood),
                ],
            }],
            post_play: PostPlayDestination::LeftNeighborHand,
        },
        occ_def("occ1"),
        occ_def("occ2"),
        occ_def("occ3"),
        occ_def("occ4"),
    ]);
    let mut player = Player::new(0, 0);
    let mut hand = vec![
        "B005".to_owned(),
        "occ1".to_owned(),
        "occ2".to_owned(),
        "occ3".to_owned(),
        "occ4".to_owned(),
    ];
    let _ = dispatch_card(&set, &mut player, &mut hand, "B005");
    assert_eq!(player.supply.stone, 1, "4 occupations -> stone");
}

#[test]
fn b005_5_occupations_grants_reed() {
    let set = new_set(vec![
        CardDefinition {
            use_limit: None,
            id: "B005".to_owned(),
            kind: CardKind::MinorImprovement,
            cost: vec![],
            points: 0,
            effects: vec![CardEffect::OnPlayGainByHandOccupationCount {
                bands: vec![
                    (4, Good::Stone),
                    (5, Good::Reed),
                    (6, Good::Clay),
                    (7, Good::Wood),
                ],
            }],
            post_play: PostPlayDestination::LeftNeighborHand,
        },
        occ_def("occ1"),
        occ_def("occ2"),
        occ_def("occ3"),
        occ_def("occ4"),
        occ_def("occ5"),
    ]);
    let mut player = Player::new(0, 0);
    let mut hand = vec![
        "B005".to_owned(),
        "occ1".to_owned(),
        "occ2".to_owned(),
        "occ3".to_owned(),
        "occ4".to_owned(),
        "occ5".to_owned(),
    ];
    let _ = dispatch_card(&set, &mut player, &mut hand, "B005");
    assert_eq!(player.supply.reed, 1, "5 occupations -> reed");
}

#[test]
fn b005_6_occupations_grants_clay() {
    let set = new_set(vec![
        CardDefinition {
            use_limit: None,
            id: "B005".to_owned(),
            kind: CardKind::MinorImprovement,
            cost: vec![],
            points: 0,
            effects: vec![CardEffect::OnPlayGainByHandOccupationCount {
                bands: vec![
                    (4, Good::Stone),
                    (5, Good::Reed),
                    (6, Good::Clay),
                    (7, Good::Wood),
                ],
            }],
            post_play: PostPlayDestination::LeftNeighborHand,
        },
        occ_def("occ1"),
        occ_def("occ2"),
        occ_def("occ3"),
        occ_def("occ4"),
        occ_def("occ5"),
        occ_def("occ6"),
    ]);
    let mut player = Player::new(0, 0);
    let mut hand = vec![
        "B005".to_owned(),
        "occ1".to_owned(),
        "occ2".to_owned(),
        "occ3".to_owned(),
        "occ4".to_owned(),
        "occ5".to_owned(),
        "occ6".to_owned(),
    ];
    let _ = dispatch_card(&set, &mut player, &mut hand, "B005");
    assert_eq!(player.supply.clay, 1, "6 occupations -> clay");
}

#[test]
fn b005_7_occupations_grants_wood() {
    let set = new_set(vec![
        CardDefinition {
            use_limit: None,
            id: "B005".to_owned(),
            kind: CardKind::MinorImprovement,
            cost: vec![],
            points: 0,
            effects: vec![CardEffect::OnPlayGainByHandOccupationCount {
                bands: vec![
                    (4, Good::Stone),
                    (5, Good::Reed),
                    (6, Good::Clay),
                    (7, Good::Wood),
                ],
            }],
            post_play: PostPlayDestination::LeftNeighborHand,
        },
        occ_def("occ1"),
        occ_def("occ2"),
        occ_def("occ3"),
        occ_def("occ4"),
        occ_def("occ5"),
        occ_def("occ6"),
        occ_def("occ7"),
    ]);
    let mut player = Player::new(0, 0);
    let mut hand = vec![
        "B005".to_owned(),
        "occ1".to_owned(),
        "occ2".to_owned(),
        "occ3".to_owned(),
        "occ4".to_owned(),
        "occ5".to_owned(),
        "occ6".to_owned(),
        "occ7".to_owned(),
    ];
    let _ = dispatch_card(&set, &mut player, &mut hand, "B005");
    assert_eq!(player.supply.wood, 1, "7 occupations -> wood");
}

#[test]
fn b005_8_occupations_grants_nothing() {
    let set = new_set(vec![
        CardDefinition {
            use_limit: None,
            id: "B005".to_owned(),
            kind: CardKind::MinorImprovement,
            cost: vec![],
            points: 0,
            effects: vec![CardEffect::OnPlayGainByHandOccupationCount {
                bands: vec![
                    (4, Good::Stone),
                    (5, Good::Reed),
                    (6, Good::Clay),
                    (7, Good::Wood),
                ],
            }],
            post_play: PostPlayDestination::LeftNeighborHand,
        },
        occ_def("occ1"),
        occ_def("occ2"),
        occ_def("occ3"),
        occ_def("occ4"),
        occ_def("occ5"),
        occ_def("occ6"),
        occ_def("occ7"),
        occ_def("occ8"),
    ]);
    let mut player = Player::new(0, 0);
    let mut hand = vec![
        "B005".to_owned(),
        "occ1".to_owned(),
        "occ2".to_owned(),
        "occ3".to_owned(),
        "occ4".to_owned(),
        "occ5".to_owned(),
        "occ6".to_owned(),
        "occ7".to_owned(),
        "occ8".to_owned(),
    ];
    let _ = dispatch_card(&set, &mut player, &mut hand, "B005");
    assert_eq!(player.supply.stone, 0);
    assert_eq!(player.supply.reed, 0);
    assert_eq!(player.supply.clay, 0);
    assert_eq!(player.supply.wood, 0);
}

// ---------------- B008 露店 (Market Stall) ----------------

#[test]
fn b008_normal_exchange_grain_for_vegetable() {
    let set = new_set(vec![CardDefinition {
        use_limit: None,
        id: "B008".to_owned(),
        kind: CardKind::MinorImprovement,
        cost: vec![(Good::Grain, 1)],
        points: 0,
        effects: vec![CardEffect::OnPlayExchangeGain {
            give: Good::Grain,
            give_amount: 1,
            receive: Good::Vegetable,
            receive_amount: 1,
        }],
        post_play: PostPlayDestination::LeftNeighborHand,
    }]);
    let mut player = Player::new(0, 0);
    player.supply.grain = 2; // 1 for cost, 1 for exchange
                             // Pay the card cost first (apply_on_play_effects doesn't pay costs)
    player.supply.pay(Good::Grain, 1).unwrap();
    apply_on_play_effects(&set, &mut player, "B008").expect("apply B008");
    assert_eq!(
        player.supply.grain, 0,
        "1 grain paid for cost, 1 for exchange -> 0 left"
    );
    assert_eq!(player.supply.vegetable, 1, "received 1 vegetable");
}

#[test]
fn b008_not_triggered_when_no_grain() {
    let set = new_set(vec![CardDefinition {
        use_limit: None,
        id: "B008".to_owned(),
        kind: CardKind::MinorImprovement,
        cost: vec![(Good::Grain, 1)],
        points: 0,
        effects: vec![CardEffect::OnPlayExchangeGain {
            give: Good::Grain,
            give_amount: 1,
            receive: Good::Vegetable,
            receive_amount: 1,
        }],
        post_play: PostPlayDestination::LeftNeighborHand,
    }]);
    let mut player = Player::new(0, 0);
    player.supply.grain = 1; // Only enough for cost
                             // Pay the card cost first (apply_on_play_effects doesn't pay costs)
    player.supply.pay(Good::Grain, 1).unwrap();
    apply_on_play_effects(&set, &mut player, "B008").expect("apply B008");
    // Exchange doesn't fire (no grain left after paying cost)
    assert_eq!(player.supply.grain, 0, "grain paid for cost");
    assert_eq!(player.supply.vegetable, 0, "no exchange -> no vegetable");
}

// ---------------- B009 むち (Beating Rod) ----------------

#[test]
fn b009_normal_gain_reed() {
    let set = new_set(vec![CardDefinition {
        use_limit: None,
        id: "B009".to_owned(),
        kind: CardKind::MinorImprovement,
        cost: vec![],
        points: 0,
        effects: vec![CardEffect::OnPlayGainOrExchangeAnimal {
            gain_good: Good::Reed,
            gain_amount: 1,
            pay_good: Good::Reed,
            pay_amount: 1,
            animal: Animal::Cattle,
        }],
        post_play: PostPlayDestination::LeftNeighborHand,
    }]);
    let mut player = Player::new(0, 0);
    player.supply.reed = 0;
    apply_on_play_effects(&set, &mut player, "B009").expect("apply B009");
    assert_eq!(player.supply.reed, 1, "no reed -> gain 1 reed");
    assert_eq!(player.animal_count(Animal::Cattle), 0);
}

#[test]
fn b009_exchange_reed_for_cattle() {
    let set = new_set(vec![CardDefinition {
        use_limit: None,
        id: "B009".to_owned(),
        kind: CardKind::MinorImprovement,
        cost: vec![],
        points: 0,
        effects: vec![CardEffect::OnPlayGainOrExchangeAnimal {
            gain_good: Good::Reed,
            gain_amount: 1,
            pay_good: Good::Reed,
            pay_amount: 1,
            animal: Animal::Cattle,
        }],
        post_play: PostPlayDestination::LeftNeighborHand,
    }]);
    let mut player = Player::new(0, 0);
    player.supply.reed = 1;
    apply_on_play_effects(&set, &mut player, "B009").expect("apply B009");
    assert_eq!(player.supply.reed, 0, "1 reed paid for cattle");
    assert_eq!(player.animal_count(Animal::Cattle), 1);
}

// ---------------- cost / prerequisite regression pins ----------------

#[test]
fn b002_cost_food_2_pinned() {
    let registry = build_default_registry().unwrap();
    let card_id = registry.card_id("B002").unwrap();
    let card = registry.get(card_id).unwrap();
    assert_eq!(card.cost, vec![(Good::Food, 2)], "B002 cost must be food 2");
}

#[test]
fn b003_cost_zero_pinned() {
    let registry = build_default_registry().unwrap();
    let card_id = registry.card_id("B003").unwrap();
    let card = registry.get(card_id).unwrap();
    assert_eq!(card.cost, vec![], "B003 cost must be free");
}

#[test]
fn b005_cost_zero_pinned() {
    let registry = build_default_registry().unwrap();
    let card_id = registry.card_id("B005").unwrap();
    let card = registry.get(card_id).unwrap();
    assert_eq!(card.cost, vec![], "B005 cost must be free");
}

#[test]
fn b008_cost_grain_1_pinned() {
    let registry = build_default_registry().unwrap();
    let card_id = registry.card_id("B008").unwrap();
    let card = registry.get(card_id).unwrap();
    assert_eq!(
        card.cost,
        vec![(Good::Grain, 1)],
        "B008 cost must be grain 1"
    );
}

#[test]
fn b009_cost_zero_pinned() {
    let registry = build_default_registry().unwrap();
    let card_id = registry.card_id("B009").unwrap();
    let card = registry.get(card_id).unwrap();
    assert_eq!(card.cost, vec![], "B009 cost must be free");
}

#[test]
fn spec_cost_parity_with_verified_cards() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    for id in ["B002", "B003", "B005", "B008", "B009"] {
        let path = root
            .join("data")
            .join("bga-card-specs")
            .join(format!("{id}.json"));
        let text = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
        let spec: serde_json::Value = serde_json::from_str(&text).unwrap();
        let registry = build_default_registry().unwrap();
        let card_id = registry.card_id(id).unwrap();
        let card = registry.get(card_id).unwrap();

        let mut spec_cost: Vec<(String, u64)> = spec["play"]["cost"]
            .as_array()
            .unwrap_or_else(|| panic!("{id} spec cost must be an array"))
            .iter()
            .map(|e| {
                (
                    e["good"].as_str().unwrap().to_string(),
                    e["amount"].as_u64().unwrap(),
                )
            })
            .collect();
        spec_cost.sort();
        let mut impl_cost: Vec<(String, u64)> = card
            .cost
            .iter()
            .map(|(good, amount)| (format!("{good:?}").to_lowercase(), u64::from(*amount)))
            .collect();
        impl_cost.sort();
        assert_eq!(
            impl_cost, spec_cost,
            "{id}: cost parity spec vs implementation"
        );

        let prerequisites = spec["play"]["prerequisites"]
            .as_array()
            .unwrap_or_else(|| panic!("{id} spec prerequisites must be an array"));
        assert!(
            prerequisites.is_empty(),
            "{id}: spec must have no prerequisites"
        );
        assert!(
            !card
                .effects
                .iter()
                .any(|e| matches!(e, CardEffect::Prerequisite { .. })),
            "{id}: implementation must carry no prerequisite"
        );
    }
}

// ---------------- pass-left regression test ----------------

#[test]
fn pass_left_zone_transition_and_card_points() {
    // Test that pass-left cards (B003) correctly transition from owner's
    // played area to left neighbor's hand via the canonical move_card API,
    // and that card_points is updated correctly (points added then subtracted).
    // This verifies the fix for P1-1 (regression test) and P1-2 (move_card API).

    let set = new_set(vec![
        CardDefinition {
            use_limit: None,
            id: "B003".to_owned(),
            kind: CardKind::MinorImprovement,
            cost: vec![],
            points: 0, // B003 has 0 points, but test the mechanism
            effects: vec![CardEffect::OnPlayDrawPlayOrPassOccupation { food_cost: 2 }],
            post_play: PostPlayDestination::LeftNeighborHand,
        },
        occ_def("test_occ"),
    ]);
    let mut player = Player::new(0, 0);
    player.supply.food = 0; // Force pass branch
                            // B003 is already in played_cards (simulating state after hand->played move)
    let hand = vec!["test_occ".to_owned()]; // B003 NOT in hand

    // Need a proper CardState with two hands to test left-neighbor transfer
    let mut cards = agricola_re_ai::cards::CardState {
        hands: vec![hand.clone(), vec![]], // player 0 hand, player 1 hand
        ..Default::default()
    };
    let mut players = vec![player.clone(), Player::new(1, 0)];
    // Pre-place B003 in played_cards as dispatch_play expects
    players[0].played_cards.push("B003".to_owned());
    let mut market = vec![];
    let mut well = std::collections::BTreeMap::new();
    let mut budget = 64u32;
    let mut uses = std::collections::BTreeMap::new();
    let limits = std::collections::BTreeMap::new();
    let mut pending = None;

    // Call dispatch_play directly to test the post-play block
    dispatch_play(
        &set,
        0,
        "B003",
        0,
        &mut cards,
        &mut players,
        &mut market,
        &mut well,
        1,
        &std::collections::BTreeSet::new(),
        &mut budget,
        &mut uses,
        &limits,
        &mut pending,
    )
    .expect("dispatch_play B003");

    // Verify the pass-left card left player 0's played area
    assert!(
        !players[0].played_cards.contains(&"B003".to_string()),
        "B003 should leave player 0's played area"
    );
    // Verify it reached player 1's hand (left neighbor)
    assert!(
        cards.hands[1].contains(&"B003".to_string()),
        "B003 should reach left neighbor's hand"
    );
    // Verify card_points: B003 has 0 points, so no change expected
    // but the mechanism should not panic
    assert_eq!(
        players[0].card_points, 0,
        "card_points should be 0 for B003"
    );

    // Verify card partition invariant (no duplicates, no missing cards)
    agricola_re_ai::card_zones::validate_card_partition(&set, &cards, &players)
        .expect("card partition must remain valid after pass-left");
}

#[test]
fn pass_left_with_nonzero_points_subtraction() {
    // Test that a pass-left card with non-zero points correctly adds
    // then subtracts points from card_points. This prevents the
    // potential underflow bug described in P2-1.
    let set = new_set(vec![CardDefinition {
        use_limit: None,
        id: "TEST_PASS_LEFT".to_owned(),
        kind: CardKind::MinorImprovement,
        cost: vec![],
        points: 2, // Non-zero points
        effects: vec![],
        post_play: PostPlayDestination::LeftNeighborHand,
    }]);
    let player = Player::new(0, 0);
    // Card NOT in hand - already in played_cards
    let mut cards = agricola_re_ai::cards::CardState {
        hands: vec![vec![], vec![]],
        ..Default::default()
    };
    let mut players = vec![player.clone(), Player::new(1, 0)];
    players[0].played_cards.push("TEST_PASS_LEFT".to_owned());
    let mut market = vec![];
    let mut well = std::collections::BTreeMap::new();
    let mut budget = 64u32;
    let mut uses = std::collections::BTreeMap::new();
    let limits = std::collections::BTreeMap::new();
    let mut pending = None;

    dispatch_play(
        &set,
        0,
        "TEST_PASS_LEFT",
        0,
        &mut cards,
        &mut players,
        &mut market,
        &mut well,
        1,
        &std::collections::BTreeSet::new(),
        &mut budget,
        &mut uses,
        &limits,
        &mut pending,
    )
    .expect("dispatch_play TEST_PASS_LEFT");

    // Card has 2 points: added on play, then subtracted on pass-left
    // Net should be 0
    assert_eq!(
        players[0].card_points, 0,
        "card_points should be 0 after add+subtract for pass-left card with points"
    );
    assert!(
        !players[0]
            .played_cards
            .contains(&"TEST_PASS_LEFT".to_string()),
        "card should leave played area"
    );
    assert!(
        cards.hands[1].contains(&"TEST_PASS_LEFT".to_string()),
        "card should reach left neighbor"
    );

    agricola_re_ai::card_zones::validate_card_partition(&set, &cards, &players)
        .expect("card partition must remain valid");
}

#[test]
fn pass_left_requires_two_players() {
    // Test that pass-left is a no-op in 1-player games (P2-4 guard).
    // The card should stay in the owner's played area.
    let set = new_set(vec![CardDefinition {
        use_limit: None,
        id: "B003".to_owned(),
        kind: CardKind::MinorImprovement,
        cost: vec![],
        points: 0,
        effects: vec![CardEffect::OnPlayDrawPlayOrPassOccupation { food_cost: 2 }],
        post_play: PostPlayDestination::LeftNeighborHand,
    }]);
    let mut player = Player::new(0, 0);
    player.supply.food = 0; // Force pass branch
                            // B003 NOT in hand - already in played_cards
    let mut cards = agricola_re_ai::cards::CardState {
        hands: vec![vec!["test_occ".to_owned()]], // Only 1 hand, B003 not in it
        ..Default::default()
    };
    let mut players = vec![player.clone()]; // Only 1 player
    players[0].played_cards.push("B003".to_owned());
    let mut market = vec![];
    let mut well = std::collections::BTreeMap::new();
    let mut budget = 64u32;
    let mut uses = std::collections::BTreeMap::new();
    let limits = std::collections::BTreeMap::new();
    let mut pending = None;

    // Should not panic with 1 player
    dispatch_play(
        &set,
        0,
        "B003",
        0,
        &mut cards,
        &mut players,
        &mut market,
        &mut well,
        1,
        &std::collections::BTreeSet::new(),
        &mut budget,
        &mut uses,
        &limits,
        &mut pending,
    )
    .expect("dispatch_play B003 in 1-player game");

    // In 1-player game, card should stay in played area
    assert!(
        players[0].played_cards.contains(&"B003".to_string()),
        "B003 should stay in played area in 1-player game"
    );
    // No left neighbor hand exists, so no transfer
    assert_eq!(cards.hands.len(), 1, "only 1 hand in 1-player game");

    agricola_re_ai::card_zones::validate_card_partition(&set, &cards, &players)
        .expect("card partition must remain valid");
}

// ---------------- P1-1: pass-left via the engine's production card-play path ----------------

/// Build a valid 2-player game whose deck satisfies `CardSet::validate` (7 of
/// each kind per player) and whose hands satisfy `from_hands` (7 occupations
/// + 7 minor improvements per seat).
///
/// `pass_left_id` is a MinorImprovement with `post_play = LeftNeighborHand`
/// placed in player 0's hand.
fn build_pass_left_game(pass_left_id: &str, points: i32, effects: Vec<CardEffect>) -> Game {
    use agricola_re_ai::model::{Config, Game};

    let dummy_mi: Vec<String> = (0..13).map(|i| format!("dummy_mi_{i}")).collect();
    let dummy_oc: Vec<String> = (0..14).map(|i| format!("dummy_oc_{i}")).collect();

    let mut mi_defs: Vec<CardDefinition> = vec![CardDefinition {
        use_limit: None,
        id: pass_left_id.to_owned(),
        kind: CardKind::MinorImprovement,
        cost: vec![],
        points,
        effects,
        post_play: PostPlayDestination::LeftNeighborHand,
    }];
    mi_defs.extend(dummy_mi.iter().map(|id| CardDefinition {
        use_limit: None,
        id: id.clone(),
        kind: CardKind::MinorImprovement,
        cost: vec![],
        points: 0,
        effects: vec![],
        post_play: PostPlayDestination::PlayedArea,
    }));

    let oc_defs: Vec<CardDefinition> = dummy_oc
        .iter()
        .map(|id| CardDefinition {
            use_limit: None,
            id: id.clone(),
            kind: CardKind::Occupation,
            cost: vec![],
            points: 0,
            effects: vec![],
            post_play: PostPlayDestination::PlayedArea,
        })
        .collect();

    let set = agricola_re_ai::cards::CardSet {
        definitions: mi_defs.into_iter().chain(oc_defs).collect(),
    };

    let hand0: Vec<String> = std::iter::once(pass_left_id.to_owned())
        .chain(dummy_mi[0..6].iter().cloned())
        .chain(dummy_oc[0..7].iter().cloned())
        .collect();
    let hand1: Vec<String> = dummy_mi[6..13]
        .iter()
        .cloned()
        .chain(dummy_oc[7..14].iter().cloned())
        .collect();

    Game::with_cards_and_hands(Config::default(), set, vec![hand0, hand1])
        .expect("game with hands builds")
}

/// Exercises `dispatch_play`'s post-play block through the engine production
/// path (`Space::MeetingPlace` → `Detail::PlayMinor` → `dispatch_play` — a
/// regular space, so no round-card reveal is needed and every
/// `Game::validate` invariant stays satisfiable). Verifies the pass-left card
/// leaves the owner's played area, reaches the left neighbour's hand,
/// card_points add/subtract is balanced, and the state-consistency invariant
/// holds (§14).
#[test]
fn pass_left_via_engine_play_minor_path() {
    use agricola_re_ai::engine::{Action, Detail};
    use agricola_re_ai::model::Space;

    let mut game = build_pass_left_game(
        "B003",
        0,
        vec![CardEffect::OnPlayDrawPlayOrPassOccupation { food_cost: 2 }],
    );
    // Open round 1 through the production path so `Game::validate`'s
    // round/deck invariants hold.
    game.start_round().expect("start round 1");
    // Give player 0 enough food to pay so the drawn occupation is played
    // (moved to played area) rather than passed to the left neighbour, keeping
    // the pass-left assertion focused on B003 itself.
    game.public.players[0].supply.food = 2;

    game.apply(Action {
        space: Space::MeetingPlace,
        detail: Detail::PlayMinor {
            card_id: "B003".to_owned(),
        },
    })
    .expect("playing pass-left card via MeetingPlace succeeds");

    // Card left owner's played area
    assert!(
        !game.public.players[0]
            .played_cards
            .contains(&"B003".to_string()),
        "B003 should leave player 0's played area"
    );
    // Card reached left neighbour's hand
    assert!(
        game.cards.hands[1].contains(&"B003".to_string()),
        "B003 should reach left neighbour's hand"
    );
    // card_points: 0 added, 0 subtracted → net 0
    assert_eq!(
        game.public.players[0].card_points, 0,
        "card_points should be 0 for B003"
    );
    // Card partition invariant (state consistency, §14)
    game.validate()
        .expect("game state must remain valid after engine pass-left");
}

/// P2-1: a pass-left card with non-zero points must add points on play then
/// subtract them on pass-left, never underflowing `card_points`.
#[test]
fn pass_left_via_engine_nonzero_points_balance() {
    use agricola_re_ai::engine::{Action, Detail};
    use agricola_re_ai::model::Space;

    let mut game = build_pass_left_game("TEST_PASS_LEFT", 2, vec![]);
    // Open round 1 through the production path (see the sibling test).
    game.start_round().expect("start round 1");

    game.apply(Action {
        space: Space::MeetingPlace,
        detail: Detail::PlayMinor {
            card_id: "TEST_PASS_LEFT".to_owned(),
        },
    })
    .expect("playing non-zero-point pass-left card via MeetingPlace succeeds");

    // Card left owner's played area
    assert!(
        !game.public.players[0]
            .played_cards
            .contains(&"TEST_PASS_LEFT".to_string()),
        "card should leave player 0's played area"
    );
    // Card reached left neighbour's hand
    assert!(
        game.cards.hands[1].contains(&"TEST_PASS_LEFT".to_string()),
        "card should reach left neighbour's hand"
    );
    // card_points: +2 on play, −2 on pass-left → net 0 (no underflow)
    assert_eq!(
        game.public.players[0].card_points, 0,
        "card_points should net to 0 after add+subtract"
    );
    // Card partition invariant (state consistency, §14)
    game.validate().expect("game state must remain valid");
}
