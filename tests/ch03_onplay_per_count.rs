//! Tests for ch03 OnPlayGainPerCount cards: C004, C005, D005, B006, B007
//! (B004 薪の山 is held as needs_resolution — see P0-3).
//!
//! Covers, per card: registry reachability, effect-only play via
//! `cards::apply_on_play_effects` (supply delta), legal-play / prerequisite
//! gating via `cards::play`/`cards::can_play` (B006), and the dispatcher
//! route via `card_events::dispatch_play` (supply increase through the
//! `card_events` effect path).
//!
//! Pinned semantics:
//! - C005 `count[improvement]` = improvements in front of the player:
//!   played minor-improvement cards + owned major improvements, the same as
//!   `cards::improvement_count_in_front` (the A117 木材調達人 precedent).
//!   The spec notes say "major improvement owned"; this repo's canonical
//!   `improvement` count covers all improvements in front (see PR body P2-8).
//! - B007 bottom row = {Joinery, Pottery, Basketry}: the bottom row of the
//!   major-improvement supply board (3 good-conversion improvements), pinned
//!   by B007-normal (bottom row 2 -> food_delta 4). Composition is not
//!   derivable from repo-internal spec data alone (see PR body).

use agricola_re_ai::card_events::dispatch_play;
use agricola_re_ai::cards::{
    apply_on_play_effects, can_play, play, CardDefinition, CardEffect, CardKind, CardRequirement,
    CardSet,
};
use agricola_re_ai::model::{Crop, Good, House, Improvement, Player, Tile};
use agricola_re_ai::registry::build_default_registry;
use std::collections::{BTreeMap, BTreeSet};

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
    }
}

fn minor_def(id: &str) -> CardDefinition {
    CardDefinition {
        use_limit: None,
        id: id.to_owned(),
        kind: CardKind::MinorImprovement,
        cost: vec![],
        points: 0,
        effects: vec![],
    }
}

/// Dispatcher route: pay the printed cost manually (dispatch_play does not
/// validate cost), then run `dispatch_play` for card `id`.
fn dispatch_card(set: &CardSet, player: &mut Player, hand: &mut [String], id: &str) {
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
    let mut well = BTreeMap::new();
    let mut budget = 64u32;
    let mut uses = BTreeMap::new();
    let limits = BTreeMap::new();
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
        &BTreeSet::new(),
        &mut budget,
        &mut uses,
        &limits,
        &mut pending,
    )
    .unwrap_or_else(|e| panic!("dispatch_play {id}: {e}"));
    *player = players[0].clone();
}

// ---------------- Registry reachability ----------------

#[test]
fn ch03_cards_reachable_via_default_registry() {
    let registry = build_default_registry().expect("default registry builds");
    for id in ["C004", "C005", "D005", "B006", "B007"] {
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
            card.effects
                .iter()
                .any(|e| matches!(e, CardEffect::OnPlayGainPerCount { .. })),
            "{id} must carry OnPlayGainPerCount"
        );
    }
    // B004 must NOT be registered (held as needs_resolution).
    assert!(
        registry.card_id("B004").is_none(),
        "B004 must not be registered"
    );
}

// ---------------- C004 筆記板 ----------------

#[test]
fn c004_normal_occupations_3_wood_delta_3() {
    let set = new_set(vec![
        CardDefinition {
            use_limit: None,
            id: "C004".to_owned(),
            kind: CardKind::MinorImprovement,
            cost: vec![(Good::Food, 1)],
            points: 0,
            effects: vec![CardEffect::OnPlayGainPerCount {
                count_kind: agricola_re_ai::cards::CountKind::Occupations,
                good: Good::Wood,
                amount: 1,
            }],
        },
        occ_def("occ1"),
        occ_def("occ2"),
        occ_def("occ3"),
    ]);
    let mut player = Player::new(0, 0);
    player.played_cards = vec!["occ1".into(), "occ2".into(), "occ3".into()];
    apply_on_play_effects(&set, &mut player, "C004").expect("apply C004");
    assert_eq!(player.supply.wood, 3, "3 occupations -> wood_delta 3");
}

#[test]
fn c004_zero_occupations_0_wood_delta_0() {
    let set = new_set(vec![CardDefinition {
        use_limit: None,
        id: "C004".to_owned(),
        kind: CardKind::MinorImprovement,
        cost: vec![(Good::Food, 1)],
        points: 0,
        effects: vec![CardEffect::OnPlayGainPerCount {
            count_kind: agricola_re_ai::cards::CountKind::Occupations,
            good: Good::Wood,
            amount: 1,
        }],
    }]);
    let mut player = Player::new(0, 0);
    apply_on_play_effects(&set, &mut player, "C004").expect("apply C004");
    assert_eq!(player.supply.wood, 0, "0 occupations -> wood_delta 0");
}

#[test]
fn c004_dispatcher_route_grants_wood_per_occupation() {
    let set = new_set(vec![
        CardDefinition {
            use_limit: None,
            id: "C004".to_owned(),
            kind: CardKind::MinorImprovement,
            cost: vec![(Good::Food, 1)],
            points: 0,
            effects: vec![CardEffect::OnPlayGainPerCount {
                count_kind: agricola_re_ai::cards::CountKind::Occupations,
                good: Good::Wood,
                amount: 1,
            }],
        },
        occ_def("occ1"),
        occ_def("occ2"),
    ]);
    let mut player = Player::new(0, 0);
    player.played_cards = vec!["occ1".into(), "occ2".into()];
    let mut hand = vec!["C004".to_owned()];
    player.supply.food = 1;
    dispatch_card(&set, &mut player, &mut hand, "C004");
    assert_eq!(
        player.supply.wood, 2,
        "dispatcher: 2 occupations -> wood_delta 2"
    );
}

// ---------------- C005 改修 ----------------

#[test]
fn c005_normal_clay_rooms_2_plus_improvement_1_clay_delta_3() {
    let set = new_set(vec![CardDefinition {
        use_limit: None,
        id: "C005".to_owned(),
        kind: CardKind::MinorImprovement,
        cost: vec![(Good::Food, 1)],
        points: 0,
        effects: vec![
            CardEffect::OnPlayGainPerCount {
                count_kind: agricola_re_ai::cards::CountKind::ClayRooms,
                good: Good::Clay,
                amount: 1,
            },
            CardEffect::OnPlayGainPerCount {
                count_kind: agricola_re_ai::cards::CountKind::Improvements,
                good: Good::Clay,
                amount: 1,
            },
        ],
    }]);
    let mut player = Player::new(0, 0);
    player.house = House::Clay; // default 2 rooms -> 2 clay rooms
    player.improvements.push(Improvement::Well); // 1 improvement in front
    apply_on_play_effects(&set, &mut player, "C005").expect("apply C005");
    assert_eq!(
        player.supply.clay, 3,
        "clay_rooms 2 + improvement 1 -> clay_delta 3"
    );
}

#[test]
fn c005_improvement_counts_played_minor_plus_major() {
    // P2-8 decision: `count[improvement]` = all improvements in front
    // (played minor + owned major), per `improvement_count_in_front`.
    let set = new_set(vec![
        CardDefinition {
            use_limit: None,
            id: "C005".to_owned(),
            kind: CardKind::MinorImprovement,
            cost: vec![(Good::Food, 1)],
            points: 0,
            effects: vec![CardEffect::OnPlayGainPerCount {
                count_kind: agricola_re_ai::cards::CountKind::Improvements,
                good: Good::Clay,
                amount: 1,
            }],
        },
        minor_def("minor1"),
    ]);
    let mut player = Player::new(0, 0);
    player.house = House::Clay;
    player.improvements.push(Improvement::Well); // 1 major
    player.played_cards.push("minor1".into()); // 1 minor played
    apply_on_play_effects(&set, &mut player, "C005").expect("apply C005");
    assert_eq!(
        player.supply.clay, 2,
        "1 major + 1 minor -> improvement count 2 -> clay_delta 2"
    );
}

#[test]
fn c005_zero_clay_delta_0() {
    let set = new_set(vec![CardDefinition {
        use_limit: None,
        id: "C005".to_owned(),
        kind: CardKind::MinorImprovement,
        cost: vec![(Good::Food, 1)],
        points: 0,
        effects: vec![CardEffect::OnPlayGainPerCount {
            count_kind: agricola_re_ai::cards::CountKind::ClayRooms,
            good: Good::Clay,
            amount: 1,
        }],
    }]);
    let mut player = Player::new(0, 0); // wood house -> 0 clay rooms
    apply_on_play_effects(&set, &mut player, "C005").expect("apply C005");
    assert_eq!(player.supply.clay, 0, "0 clay rooms -> clay_delta 0");
}

// ---------------- D005 畑の粘土 ----------------

#[test]
fn d005_normal_sown_fields_2_clay_delta_2() {
    let set = new_set(vec![CardDefinition {
        use_limit: None,
        id: "D005".to_owned(),
        kind: CardKind::MinorImprovement,
        cost: vec![(Good::Food, 1)],
        points: 0,
        effects: vec![CardEffect::OnPlayGainPerCount {
            count_kind: agricola_re_ai::cards::CountKind::SownFields,
            good: Good::Clay,
            amount: 1,
        }],
    }]);
    let mut player = Player::new(0, 0);
    player.farm[1].tile = Tile::Field {
        crop: Some(Crop::Grain),
        remaining: 1,
    };
    player.farm[2].tile = Tile::Field {
        crop: Some(Crop::Vegetable),
        remaining: 1,
    };
    apply_on_play_effects(&set, &mut player, "D005").expect("apply D005");
    assert_eq!(player.supply.clay, 2, "2 sown fields -> clay_delta 2");
}

#[test]
fn d005_zero_clay_delta_0() {
    let set = new_set(vec![CardDefinition {
        use_limit: None,
        id: "D005".to_owned(),
        kind: CardKind::MinorImprovement,
        cost: vec![(Good::Food, 1)],
        points: 0,
        effects: vec![CardEffect::OnPlayGainPerCount {
            count_kind: agricola_re_ai::cards::CountKind::SownFields,
            good: Good::Clay,
            amount: 1,
        }],
    }]);
    let mut player = Player::new(0, 0);
    apply_on_play_effects(&set, &mut player, "D005").expect("apply D005");
    assert_eq!(player.supply.clay, 0, "0 sown fields -> clay_delta 0");
}

#[test]
fn d005_dispatcher_route_grants_clay_per_sown_field() {
    let set = new_set(vec![CardDefinition {
        use_limit: None,
        id: "D005".to_owned(),
        kind: CardKind::MinorImprovement,
        cost: vec![(Good::Food, 1)],
        points: 0,
        effects: vec![CardEffect::OnPlayGainPerCount {
            count_kind: agricola_re_ai::cards::CountKind::SownFields,
            good: Good::Clay,
            amount: 1,
        }],
    }]);
    let mut player = Player::new(0, 0);
    player.farm[3].tile = Tile::Field {
        crop: Some(Crop::Grain),
        remaining: 1,
    };
    player.supply.food = 1;
    let mut hand = vec!["D005".to_owned()];
    dispatch_card(&set, &mut player, &mut hand, "D005");
    assert_eq!(
        player.supply.clay, 1,
        "dispatcher: 1 sown field -> clay_delta 1"
    );
}

// ---------------- B006 採石場見学 ----------------

#[test]
fn b006_normal_family_3_stone_delta_3() {
    let set = new_set(vec![CardDefinition {
        use_limit: None,
        id: "B006".to_owned(),
        kind: CardKind::MinorImprovement,
        cost: vec![(Good::Food, 2)],
        points: 0,
        effects: vec![
            CardEffect::Prerequisite {
                rule: CardRequirement::MinimumOccupations(1),
            },
            CardEffect::OnPlayGainPerCount {
                count_kind: agricola_re_ai::cards::CountKind::FamilyMembers,
                good: Good::Stone,
                amount: 1,
            },
        ],
    }]);
    let mut player = Player::new(0, 2);
    player.family = 3;
    player.played_cards.push("occ1".into()); // satisfies prerequisite
    apply_on_play_effects(&set, &mut player, "B006").expect("apply B006");
    assert_eq!(player.supply.stone, 3, "3 family members -> stone_delta 3");
}

#[test]
fn b006_boundary_family_2_stone_delta_2() {
    let set = new_set(vec![CardDefinition {
        use_limit: None,
        id: "B006".to_owned(),
        kind: CardKind::MinorImprovement,
        cost: vec![(Good::Food, 2)],
        points: 0,
        effects: vec![CardEffect::OnPlayGainPerCount {
            count_kind: agricola_re_ai::cards::CountKind::FamilyMembers,
            good: Good::Stone,
            amount: 1,
        }],
    }]);
    let mut player = Player::new(0, 0);
    apply_on_play_effects(&set, &mut player, "B006").expect("apply B006");
    assert_eq!(player.supply.stone, 2, "2 family members -> stone_delta 2");
}

#[test]
fn b006_prerequisite_unmet_cannot_play() {
    let set = new_set(vec![CardDefinition {
        use_limit: None,
        id: "B006".to_owned(),
        kind: CardKind::MinorImprovement,
        cost: vec![(Good::Food, 2)],
        points: 0,
        effects: vec![
            CardEffect::Prerequisite {
                rule: CardRequirement::MinimumOccupations(1),
            },
            CardEffect::OnPlayGainPerCount {
                count_kind: agricola_re_ai::cards::CountKind::FamilyMembers,
                good: Good::Stone,
                amount: 1,
            },
        ],
    }]);
    let card = set.definition("B006").unwrap();
    let mut player = Player::new(0, 10);
    // No occupations -> prerequisite unmet -> cannot play
    assert!(
        !can_play(&set, card, &player, 0),
        "0 occupations -> B006 unplayable"
    );
    let mut hand = vec!["B006".to_owned()];
    assert!(
        play(&set, &mut hand, &mut player, "B006", card.kind, 0).is_err(),
        "play must fail when prerequisite unmet"
    );
    assert_eq!(player.supply.stone, 0, "no gain when unplayable");
}

#[test]
fn b006_prerequisite_met_can_play() {
    let set = new_set(vec![
        CardDefinition {
            use_limit: None,
            id: "B006".to_owned(),
            kind: CardKind::MinorImprovement,
            cost: vec![(Good::Food, 2)],
            points: 0,
            effects: vec![CardEffect::OnPlayGainPerCount {
                count_kind: agricola_re_ai::cards::CountKind::FamilyMembers,
                good: Good::Stone,
                amount: 1,
            }],
        },
        occ_def("occ1"),
    ]);
    let card = set.definition("B006").unwrap();
    let mut player = Player::new(0, 10);
    player.played_cards.push("occ1".into());
    assert!(
        can_play(&set, card, &player, 0),
        "1 occupation -> B006 playable"
    );
}

#[test]
fn b006_dispatcher_route_prerequisite_unmet() {
    let set = new_set(vec![CardDefinition {
        use_limit: None,
        id: "B006".to_owned(),
        kind: CardKind::MinorImprovement,
        cost: vec![(Good::Food, 2)],
        points: 0,
        effects: vec![
            CardEffect::Prerequisite {
                rule: CardRequirement::MinimumOccupations(1),
            },
            CardEffect::OnPlayGainPerCount {
                count_kind: agricola_re_ai::cards::CountKind::FamilyMembers,
                good: Good::Stone,
                amount: 1,
            },
        ],
    }]);
    let mut player = Player::new(0, 5);
    let mut hand = vec!["B006".to_owned()];
    // Cost paid manually, but prerequisite unmet -> dispatcher rejects the play
    let result = dispatch_card_result(&set, &mut player, &mut hand, "B006");
    assert!(
        result.is_err(),
        "dispatcher must reject B006 without 1 occupation"
    );
}

fn dispatch_card_result(
    set: &CardSet,
    player: &mut Player,
    hand: &mut [String],
    id: &str,
) -> Result<(), String> {
    let card = set
        .definition(id)
        .unwrap_or_else(|| panic!("{id} missing from set"));
    for (good, amount) in &card.cost {
        player.supply.pay(*good, *amount)?;
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
    let mut well = BTreeMap::new();
    let mut budget = 64u32;
    let mut uses = BTreeMap::new();
    let limits = BTreeMap::new();
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
        &BTreeSet::new(),
        &mut budget,
        &mut uses,
        &limits,
        &mut pending,
    )
}

#[test]
fn b006_dispatcher_route_family_3_stone_delta_3() {
    let set = new_set(vec![
        CardDefinition {
            use_limit: None,
            id: "B006".to_owned(),
            kind: CardKind::MinorImprovement,
            cost: vec![(Good::Food, 2)],
            points: 0,
            effects: vec![
                CardEffect::Prerequisite {
                    rule: CardRequirement::MinimumOccupations(1),
                },
                CardEffect::OnPlayGainPerCount {
                    count_kind: agricola_re_ai::cards::CountKind::FamilyMembers,
                    good: Good::Stone,
                    amount: 1,
                },
            ],
        },
        occ_def("occ1"),
    ]);
    let mut player = Player::new(0, 5);
    player.family = 3;
    player.played_cards.push("occ1".into());
    let mut hand = vec!["B006".to_owned()];
    dispatch_card(&set, &mut player, &mut hand, "B006");
    assert_eq!(
        player.supply.stone, 3,
        "dispatcher: family 3 -> stone_delta 3"
    );
}

// ---------------- B007 賃金 ----------------

#[test]
fn b007_normal_bottom_row_2_food_delta_4() {
    let set = new_set(vec![CardDefinition {
        use_limit: None,
        id: "B007".to_owned(),
        kind: CardKind::MinorImprovement,
        cost: vec![],
        points: 0,
        effects: vec![
            CardEffect::OnPlayGain {
                good: Good::Food,
                amount: 2,
            },
            CardEffect::OnPlayGainPerCount {
                count_kind: agricola_re_ai::cards::CountKind::MajorImprovementsBottomRow,
                good: Good::Food,
                amount: 1,
            },
        ],
    }]);
    let mut player = Player::new(0, 0);
    player.improvements.push(Improvement::Joinery);
    player.improvements.push(Improvement::Pottery); // bottom row = 2
    apply_on_play_effects(&set, &mut player, "B007").expect("apply B007");
    assert_eq!(player.supply.food, 4, "bottom row 2 -> food_delta 4");
}

#[test]
fn b007_zero_food_delta_2() {
    let set = new_set(vec![CardDefinition {
        use_limit: None,
        id: "B007".to_owned(),
        kind: CardKind::MinorImprovement,
        cost: vec![],
        points: 0,
        effects: vec![CardEffect::OnPlayGain {
            good: Good::Food,
            amount: 2,
        }],
    }]);
    let mut player = Player::new(0, 0);
    apply_on_play_effects(&set, &mut player, "B007").expect("apply B007");
    assert_eq!(player.supply.food, 2, "bottom row 0 -> food_delta 2");
}

#[test]
fn b007_dispatcher_route_bottom_row_2_food_delta_4() {
    let set = new_set(vec![CardDefinition {
        use_limit: None,
        id: "B007".to_owned(),
        kind: CardKind::MinorImprovement,
        cost: vec![],
        points: 0,
        effects: vec![
            CardEffect::OnPlayGain {
                good: Good::Food,
                amount: 2,
            },
            CardEffect::OnPlayGainPerCount {
                count_kind: agricola_re_ai::cards::CountKind::MajorImprovementsBottomRow,
                good: Good::Food,
                amount: 1,
            },
        ],
    }]);
    let mut player = Player::new(0, 0);
    player.improvements.push(Improvement::Pottery);
    player.improvements.push(Improvement::Basketry); // bottom row = 2
    let mut hand = vec!["B007".to_owned()];
    dispatch_card(&set, &mut player, &mut hand, "B007");
    assert_eq!(
        player.supply.food, 4,
        "dispatcher: bottom row 2 -> food_delta 4"
    );
}

// ---------------- cost / prerequisite regression pins (Round 2, P1-2) ----------------

fn real_defs() -> Vec<CardDefinition> {
    let mut defs = agricola_re_ai::cards::verified_band_c_cards();
    defs.extend(agricola_re_ai::cards::verified_band_d_cards());
    defs.extend(agricola_re_ai::cards::verified_band_b_cards());
    defs
}

fn real_card<'a>(defs: &'a [CardDefinition], id: &str) -> &'a CardDefinition {
    defs.iter()
        .find(|d| d.id == id)
        .unwrap_or_else(|| panic!("{id} must exist in the verified band cards"))
}

fn player_with_food(food: u32) -> Player {
    let mut player = Player::new(0, 0);
    player.supply.food = food;
    player
}

fn good_name(good: Good) -> &'static str {
    match good {
        Good::Wood => "wood",
        Good::Clay => "clay",
        Good::Reed => "reed",
        Good::Stone => "stone",
        Good::Food => "food",
        Good::Grain => "grain",
        Good::Vegetable => "vegetable",
    }
}

#[test]
fn c004_cost_food_1_pinned() {
    let defs = real_defs();
    let set = new_set(defs.clone());
    let card = real_card(&defs, "C004");
    assert_eq!(card.cost, vec![(Good::Food, 1)], "C004 cost must be food 1");
    let mut player = player_with_food(1);
    assert!(can_play(&set, card, &player, 0), "food 1 -> C004 playable");
    player.supply.food = 0;
    assert!(
        !can_play(&set, card, &player, 0),
        "food 0 -> C004 unplayable"
    );
}

#[test]
fn c005_cost_food_1_pinned() {
    let defs = real_defs();
    let set = new_set(defs.clone());
    let card = real_card(&defs, "C005");
    assert_eq!(card.cost, vec![(Good::Food, 1)], "C005 cost must be food 1");
    let mut player = player_with_food(1);
    assert!(can_play(&set, card, &player, 0), "food 1 -> C005 playable");
    player.supply.food = 0;
    assert!(
        !can_play(&set, card, &player, 0),
        "food 0 -> C005 unplayable"
    );
}

#[test]
fn d005_cost_food_1_pinned() {
    let defs = real_defs();
    let set = new_set(defs.clone());
    let card = real_card(&defs, "D005");
    assert_eq!(card.cost, vec![(Good::Food, 1)], "D005 cost must be food 1");
    // 文献の前提「栽培中の畑1」（監査 第4章で実装・仕様へ追加）。
    assert!(
        card.effects.iter().any(|e| matches!(
            e,
            CardEffect::Prerequisite {
                rule: CardRequirement::MinimumSownFields(1)
            }
        )),
        "D005 must carry MinimumSownFields(1)"
    );
    let mut player = player_with_food(1);
    assert!(
        !can_play(&set, card, &player, 0),
        "no sown field -> D005 unplayable"
    );
    player.farm[1].tile = Tile::Field {
        crop: Some(Crop::Grain),
        remaining: 1,
    };
    assert!(
        can_play(&set, card, &player, 0),
        "1 sown field + food 1 -> D005 playable"
    );
    player.supply.food = 0;
    assert!(
        !can_play(&set, card, &player, 0),
        "food 0 -> D005 unplayable"
    );
}

#[test]
fn b006_cost_and_prerequisite_pinned() {
    let defs = real_defs();
    let mut with_occ = defs.clone();
    with_occ.push(occ_def("occ1"));
    let set = new_set(with_occ);
    let card = real_card(&defs, "B006");
    assert_eq!(card.cost, vec![(Good::Food, 2)], "B006 cost must be food 2");
    assert!(
        card.effects.iter().any(|e| matches!(
            e,
            CardEffect::Prerequisite {
                rule: CardRequirement::MinimumOccupations(1)
            }
        )),
        "B006 must carry MinimumOccupations(1)"
    );

    let mut player = player_with_food(2);
    // cost met, prerequisite unmet -> cannot play
    assert!(
        !can_play(&set, card, &player, 0),
        "0 occupations -> B006 unplayable"
    );
    // prerequisite met, cost unmet -> cannot play
    player.played_cards.push("occ1".into());
    player.supply.food = 1;
    assert!(
        !can_play(&set, card, &player, 0),
        "food 1 -> B006 unplayable (cost 2)"
    );
    // both met -> playable
    player.supply.food = 2;
    assert!(
        can_play(&set, card, &player, 0),
        "food 2 + 1 occupation -> B006 playable"
    );
}

#[test]
fn b007_cost_zero_pinned() {
    let defs = real_defs();
    let set = new_set(defs.clone());
    let card = real_card(&defs, "B007");
    assert_eq!(card.cost, vec![], "B007 cost must be free");
    let player = player_with_food(0);
    assert!(
        can_play(&set, card, &player, 0),
        "food 0 -> B007 playable (cost 0)"
    );
}

#[test]
fn spec_cost_prereq_parity_with_verified_cards() {
    let defs = real_defs();
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    for id in ["C004", "C005", "D005", "B006", "B007"] {
        let path = root
            .join("data")
            .join("bga-card-specs")
            .join(format!("{id}.json"));
        let text = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
        let spec: serde_json::Value = serde_json::from_str(&text).unwrap();
        let card = real_card(&defs, id);

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
            .map(|(good, amount)| (good_name(*good).to_string(), u64::from(*amount)))
            .collect();
        impl_cost.sort();
        assert_eq!(
            impl_cost, spec_cost,
            "{id}: cost parity spec vs implementation"
        );

        let prerequisites = spec["play"]["prerequisites"]
            .as_array()
            .unwrap_or_else(|| panic!("{id} spec prerequisites must be an array"));
        if id == "D005" {
            assert_eq!(prerequisites.len(), 1, "D005: exactly one prerequisite");
            assert_eq!(
                prerequisites[0]["predicate"].as_str().unwrap(),
                "sown_field_count"
            );
            assert_eq!(prerequisites[0]["params"]["n"].as_u64().unwrap(), 1);
            assert!(card.effects.iter().any(|e| matches!(
                e,
                CardEffect::Prerequisite {
                    rule: CardRequirement::MinimumSownFields(1)
                }
            )));
        } else if id == "B006" {
            assert_eq!(prerequisites.len(), 1, "B006: exactly one prerequisite");
            assert_eq!(
                prerequisites[0]["predicate"].as_str().unwrap(),
                "occupation_count"
            );
            assert_eq!(prerequisites[0]["params"]["n"].as_u64().unwrap(), 1);
            assert!(card.effects.iter().any(|e| matches!(
                e,
                CardEffect::Prerequisite {
                    rule: CardRequirement::MinimumOccupations(1)
                }
            )));
        } else {
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
}

#[test]
fn b004_absent_from_verified_cards_and_spec_held_needs_resolution() {
    let defs = real_defs();
    assert!(
        defs.iter().all(|d| d.id != "B004"),
        "B004 must not be in the verified band cards (needs_resolution)"
    );
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("data")
        .join("bga-card-specs")
        .join("B004.json");
    let text = std::fs::read_to_string(&path).unwrap();
    let spec: serde_json::Value = serde_json::from_str(&text).unwrap();
    assert!(spec["effects"].is_null(), "B004 spec effects must be null");
    assert_eq!(
        spec["review"]["status"].as_str().unwrap(),
        "needs_resolution"
    );
    assert_eq!(
        spec["review"]["implementation_status"].as_str().unwrap(),
        "not_started"
    );
}
