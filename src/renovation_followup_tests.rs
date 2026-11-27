// Renovation follow-up tests (D081 屋根梯子 / B016 解体ハンマー / A037 大枠のこぎり).
//
// Included from `engine.rs` so the full-API pause/resume path
// (`apply_with_plans` + `resolve_pending_and_resume`) and the private
// renovation helpers are reachable. These 11 lib tests cover the dispatcher
// and definition surface end-to-end (there is no separate `tests/renovation_followup.rs`).

use crate::cards::{verified_basic_card_set, CardKind};
use crate::engine::{Action, Detail};
use crate::harvest::SimpleHarvestDetails;
use crate::model::{Config, Game, Good, House, RoundCard, Space, Tile, Player};
use crate::pending::{PendingKind, PendingResolution, Transition};

const SEED: u64 = 0x5EED_1234;

/// Play a minor-improvement card through the canonical MeetingPlace path:
/// cost is paid, the card moves hand -> played, and its on-play / passive
/// effects dispatch exactly as in production play.
fn play_minor(game: &mut Game, card_id: &str) {
    game.public.players[0].supply.wood = 1;
    game.apply(Action {
        space: Space::MeetingPlace,
        detail: Detail::PlayMinor {
            card_id: card_id.to_string(),
        },
    })
    .expect("play minor card");
}

/// Push a second card straight into the played zone (like the p1_tests
/// fixtures): card partition stays valid, its on-play effects are not run.
/// The card may live in either seat's hand.
fn play_card_by_zone_move(game: &mut Game, card_id: &str) {
    let seat = game
        .cards
        .hands
        .iter()
        .position(|hand| hand.contains(&card_id.to_string()))
        .expect("card must be in some hand");
    game.cards.hands[seat].retain(|id| id != card_id);
    game.public.players[0].played_cards.push(card_id.to_string());
}
fn build_game(target: RoundCard, minors: &[&str]) -> Game {
    let set = verified_basic_card_set(2).expect("verified 2-player set");
    let all_minors: Vec<String> = set
        .definitions
        .iter()
        .filter(|c| c.kind == CardKind::MinorImprovement)
        .map(|c| c.id.clone())
        .collect();
    let all_occs: Vec<String> = set
        .definitions
        .iter()
        .filter(|c| c.kind == CardKind::Occupation)
        .map(|c| c.id.clone())
        .collect();

    let mut hand0: Vec<String> = minors.iter().map(|s| s.to_string()).collect();
    assert!(
        hand0.len() <= 7,
        "hand0 is all minor improvements: {minors:?}"
    );
    for id in &all_minors {
        if hand0.len() >= 7 {
            break;
        }
        if !hand0.contains(id) {
            hand0.push(id.clone());
        }
    }
    assert_eq!(hand0.len(), 7);
    for id in &all_occs {
        if hand0.len() >= 14 {
            break;
        }
        hand0.push(id.clone());
    }
    assert_eq!(hand0.len(), 14);

    let mut hand1: Vec<String> = Vec::new();
    for id in &all_occs {
        let occ_count = hand1.iter().filter(|id2| {
            set.definition(id2)
                .is_some_and(|c| c.kind == CardKind::Occupation)
        }).count();
        if occ_count >= 7 {
            break;
        }
        if !hand0.contains(id) && !hand1.contains(id) {
            hand1.push(id.clone());
        }
    }
    for id in &all_minors {
        if hand1.len() >= 14 {
            break;
        }
        if !hand0.contains(id) && !hand1.contains(id) {
            hand1.push(id.clone());
        }
    }
    assert_eq!(hand1.len(), 14);

    let mut game = Game::with_cards_and_hands_with_start_player(
        Config {
            players: 2,
            seed: SEED,
            additional_action_space_enabled: false,
        },
        set,
        vec![hand0, hand1],
        0,
    )
    .expect("game");
    if game.public.revealed[0] != target {
        let index = game
            .hidden_round_deck
            .iter()
            .position(|&card| card == target)
            .expect("target round card in deck");
        let original = game.public.revealed[0];
        game.public.revealed[0] = target;
        game.hidden_round_deck[index] = original;
    }
    game
}

// ----- engine unit helpers --------------------------------------------------------

#[cfg(test)]
mod renovation_engine_unit_tests {
    use super::*;

    use crate::engine::{build_stable_with_wood, can_renovate, renovation_reed_cost};

    fn player_for_renovation(house: House, rooms: usize, reed: u32, clay: u32) -> Player {
        let mut p = Player::new(0, 0);
        p.house = house;
        // Player::new starts with rooms at farm[0] and farm[5].
        let mut extra = rooms.saturating_sub(2);
        for cell in p.farm.iter_mut().skip(1) {
            if extra == 0 {
                break;
            }
            cell.tile = Tile::Room;
            extra -= 1;
        }
        p.supply.reed = reed;
        p.supply.clay = clay;
        p
    }

    #[test]
    fn renovation_reed_cost_saturates_at_zero() {
        let base = player_for_renovation(House::Clay, 2, 0, 1);
        assert_eq!(renovation_reed_cost(&base), 1);

        let mut discounted = base.clone();
        discounted.renovation_reed_discount = 1;
        assert_eq!(renovation_reed_cost(&discounted), 0);

        let mut overdiscounted = base;
        overdiscounted.renovation_reed_discount = 255;
        assert_eq!(renovation_reed_cost(&overdiscounted), 0);
    }

    #[test]
    fn can_renovate_honours_the_discount() {
        let mut base = player_for_renovation(House::Clay, 2, 0, 0);
        base.supply.stone = 2;
        assert!(!can_renovate(&base), "no reed => renovation illegal");

        let mut discounted = base;
        discounted.renovation_reed_discount = 1;
        assert!(can_renovate(&discounted), "the 1-reed discount waives the reed");
    }

    #[test]
    fn build_stable_with_wood_charges_the_given_amount() {
        let mut p = Player::new(0, 0);
        // Default rooms at farm[0] and farm[5]; cell 6 borders room 5.
        p.supply.wood = 3;
        build_stable_with_wood(&mut p, 6, 0).expect("wood 0 stable is legal");
        assert_eq!(p.supply.wood, 3);
        assert!(p.farm[6].stable);

        let mut p2 = player_for_renovation(House::Wood, 2, 1, 0);
        p2.supply.wood = 1;
        assert!(build_stable_with_wood(&mut p2, 6, 1).is_ok());
        assert_eq!(p2.supply.wood, 0);
    }
}

// ----- game-level tests ----------------------------------------------------------

#[test]
fn d081_plays_and_renovates_with_zero_reed_gaining_stone() {
    let mut game = build_game(RoundCard::HouseRedevelopment, &["D081"]);
    play_minor(&mut game, "D081");
    game.public.players[0].supply.clay = 2;
    game.public.players[0].supply.reed = 0; // the discount waives the reed entirely
    let p = &game.public.players[0];
    assert_eq!(p.supply.wood, 0, "D081 costs wood 1");
    assert_eq!(p.family, 2, "MeetingPlace play does not grow the family");
    assert_eq!(
        p.renovation_reed_discount,
        1,
        "the PassiveRule reaches the player field"
    );
    game.public.active_player = 0;
    assert!(
        game.legal_spaces().contains(&Space::HouseRedevelopment),
        "reed 0 must still allow renovation (discount waives the reed)"
    );

    game.public.active_player = 0;
    game.apply(Action {
        space: Space::HouseRedevelopment,
        detail: Detail::Renovate {
            bonus_improvement: None,
            instant_bake: 0,
            pastures: vec![],
        },
    })
    .expect("renovation is legal at reed 0 thanks to the discount");
    let p = &game.public.players[0];
    assert_eq!(p.house, House::Clay);
    assert_eq!(p.supply.clay, 0, "wood->clay payment");
    assert_eq!(p.supply.reed, 0, "discount waives the reed payment entirely");
    assert_eq!(p.supply.stone, 1, "D081 grants 1 stone after the renovation");
    game.validate().expect("state stays valid");
}

#[test]
fn d081_discount_applies_to_clay_to_stone_upgrade_too() {
    let mut game = build_game(RoundCard::HouseRedevelopment, &["D081"]);
    play_minor(&mut game, "D081"); // wood 1 paid, the PassiveRule applies the discount
    {
        let player = &mut game.public.players[0];
        player.house = House::Clay;
        // Clay -> Stone pays stone (rooms), reed waived by D081's discount.
        player.supply.stone = 2;
        player.supply.reed = 0;
    }
    game.public.active_player = 0;
    game.apply(Action {
        space: Space::HouseRedevelopment,
        detail: Detail::Renovate {
            bonus_improvement: None,
            instant_bake: 0,
            pastures: vec![],
        },
    })
    .expect("clay->stone renovation");
    let p = &game.public.players[0];
    assert_eq!(p.house, House::Stone);
    assert_eq!(p.supply.stone, 1, "2 stone paid, 1 stone gained via D081");
    assert_eq!(p.supply.reed, 0, "discount waives the reed payment");
    game.validate().expect("state stays valid");
}

#[test]
fn d081_stays_dormant_on_room_addition_and_first_wooden_house() {
    let mut game = build_game(RoundCard::HouseRedevelopment, &["D081"]);
    play_minor(&mut game, "D081");
    // Add rooms (not a renovation: house material unchanged).
    game.public.active_player = 0;
    game.public.players[0].supply.wood = 20;
    game.public.players[0].supply.reed = 10;
    game.apply(Action {
        space: Space::FarmExpansion,
        detail: Detail::Build {
            rooms: vec![1],
            stables: vec![],
        },
    })
    .expect("room build must be legal and must not pause");
    assert!(game.pending().is_none());
    assert_eq!(game.public.players[0].supply.stone, 0, "room addition does not grant stone");
    assert_eq!(game.public.players[0].house, House::Wood);

    // Fresh game: D081 played, wooden house -> wood->clay IS a renovation
    // (material change), so stone IS granted.
    let mut game2 = build_game(RoundCard::HouseRedevelopment, &["D081"]);
    play_card_by_zone_move(&mut game2, "D081");
    game2.public.players[0].supply.clay = 2;
    game2.public.players[0].supply.reed = 1;
    game2.public.active_player = 0;
    game2
        .apply(Action {
            space: Space::HouseRedevelopment,
            detail: Detail::Renovate {
                bonus_improvement: None,
                instant_bake: 0,
                pastures: vec![],
            },
        })
        .expect("wood->clay is a renovation");
    let p2 = &game2.public.players[0];
    assert_eq!(p2.house, House::Clay);
    assert_eq!(p2.supply.reed, 0, "first wooden->clay move IS a renovation, stone granted");
    assert_eq!(p2.supply.stone, 1);
}

#[test]
fn b016_plays_and_pauses_for_the_optional_stable() {
    let mut game = build_game(RoundCard::HouseRedevelopment, &["B016"]);
    game.public.players[0].supply.food = 0;
    play_minor(&mut game, "B016");
    let p = &game.public.players[0];
    assert_eq!(p.supply.food, 1, "B016 grants 1 food on play");
    assert_eq!(p.supply.wood, 0);
    game.public.players[0].supply.clay = 2;
    game.public.players[0].supply.reed = 1;

    game.public.active_player = 0;
    let transition = game
        .apply_with_plans(
            Action {
                space: Space::HouseRedevelopment,
                detail: Detail::Renovate {
                    bonus_improvement: None,
                    instant_bake: 0,
                    pastures: vec![],
                },
            },
            None,
            None,
            &SimpleHarvestDetails,
        )
        .expect("renovation is legal");
    let Transition::Paused(pending) = transition else {
        panic!("B016 must pause the chain for the optional stable, got {transition:?}")
    };
    let PendingKind::AfterRenovationBuildStable {
        source_card,
        free_stables,
        cells,
    } = &pending.kind
    else {
        panic!("unexpected pending kind: {:?}", pending.kind)
    };
    assert_eq!(source_card, "B016");
    assert_eq!(*free_stables, 1);
    assert!(cells.contains(&2), "cell 2 must be offered as a legal stable site: {cells:?}");
    assert!(!cells.contains(&0), "a room cell is never a stable site");
    assert_eq!(pending.player, 0);
    assert!(game.pending().is_some());

    let outcome = game
        .resolve_pending_and_resume(PendingResolution::AfterRenovationBuildStable {
            cell: Some(2),
        })
        .expect("stable placement resolves");
    assert_eq!(outcome, Transition::Completed);
    let p = &game.public.players[0];
    assert!(p.farm[2].stable, "the stable must stand at cell 2");
    assert_eq!(p.supply.wood, 0, "the printed wood cost is 0");
    assert_eq!(p.house, House::Clay);
    assert_eq!(p.supply.clay, 0);
    assert!(game.pending().is_none());
    game.validate().expect("state stays valid");
}

#[test]
fn b016_decline_leaves_the_farm_unchanged() {
    let mut game = build_game(RoundCard::HouseRedevelopment, &["B016"]);
    play_minor(&mut game, "B016");
    game.public.players[0].supply.clay = 2;
    game.public.players[0].supply.reed = 1;
    game.public.active_player = 0;
    let transition = game
        .apply_with_plans(
            Action {
                space: Space::HouseRedevelopment,
                detail: Detail::Renovate {
                    bonus_improvement: None,
                    instant_bake: 0,
                    pastures: vec![],
                },
            },
            None,
            None,
            &SimpleHarvestDetails,
        )
        .expect("renovation is legal");
    let Transition::Paused(pending) = transition else {
        panic!("B016 must pause: {transition:?}")
    };
    assert!(matches!(
        pending.kind,
        PendingKind::AfterRenovationBuildStable { .. }
    ));
    let outcome = game
        .resolve_pending_and_resume(PendingResolution::AfterRenovationBuildStable { cell: None })
        .expect("declining resolves");
    assert_eq!(outcome, Transition::Completed);
    let p = &game.public.players[0];
    assert!(p.farm.iter().all(|c| !c.stable), "declining builds nothing");
    assert_eq!(p.house, House::Clay);
    assert_eq!(p.supply.wood, 0);
    assert!(game.pending().is_none());
    game.validate().expect("state stays valid");
}

#[test]
fn b016_with_full_stable_capacity_resolves_without_a_decision() {
    let mut game = build_game(RoundCard::HouseRedevelopment, &["B016"]);
    play_minor(&mut game, "B016");
    game.public.players[0].supply.clay = 2;
    game.public.players[0].supply.reed = 1;
    let player = &mut game.public.players[0];
    // Occupy all four stable sites directly.
    for cell in [2, 3, 4, 6] {
        player.farm[cell].stable = true;
    }
    assert_eq!(player.occupied_stables(), 4);

    game.public.active_player = 0;
    // One-shot apply: no legal cell, no decision channel, no pause.
    game.apply(Action {
        space: Space::HouseRedevelopment,
        detail: Detail::Renovate {
            bonus_improvement: None,
            instant_bake: 0,
            pastures: vec![],
        },
    })
    .expect("renovation still legal at capacity");
    assert!(game.pending().is_none());
    assert_eq!(game.public.players[0].occupied_stables(), 4);
}

#[test]
fn d081_and_b016_together_choose_order_then_no_second_pause() {
    // D081's passive reed discount applied through the real play path; B016's
    // printed on-play gain is covered by the dedicated B016 test, so this
    // fixture keeps the zone-move push documented above.
    let mut game = build_game(RoundCard::HouseRedevelopment, &["D081", "B016"]);
    play_minor(&mut game, "D081");
    play_card_by_zone_move(&mut game, "B016");
    game.public.players[0].supply.clay = 2;
    game.public.players[0].supply.reed = 0; // D081's discount waives the reed
    let p = &game.public.players[0];
    assert_eq!(p.renovation_reed_discount, 1);
    assert_eq!(p.rooms(), 2);
    assert_eq!(p.family, 2);
    assert_eq!(p.supply.wood, 0);

    game.public.active_player = 0;
    let transition = game
        .apply_with_plans(
            Action {
                space: Space::HouseRedevelopment,
                detail: Detail::Renovate {
                    bonus_improvement: None,
                    instant_bake: 0,
                    pastures: vec![],
                },
            },
            None,
            None,
            &SimpleHarvestDetails,
        )
        .expect("renovation with two pending effects is legal");
    let Transition::Paused(pending) = transition else {
        panic!("two simultaneous renovation effects must pause on ChooseTriggerOrder, got {transition:?}")
    };
    let PendingKind::ChooseTriggerOrder { site, triggers, .. } = &pending.kind else {
        panic!("expected ChooseTriggerOrder, got {:?}", pending.kind)
    };
    assert_eq!(*site, crate::registry::EventKind::ClayConstruction);
    assert_eq!(triggers.len(), 2, "both D081 and B016 are triggerable: {triggers:?}");

    let outcome = game
        .resolve_pending_and_resume(PendingResolution::ChooseTriggerOrder {
            chosen_order: vec![0, 1],
        })
        .expect("chosen order resolves");
    assert_eq!(
        outcome,
        Transition::Completed,
        "chosen-order execution must not pause again (documented needs_resolution)"
    );
    let p = &game.public.players[0];
    assert_eq!(p.supply.stone, 1, "D081 grants 1 stone after the renovation");
    assert!(
        p.farm.iter().all(|c| !c.stable),
        "the optional wood-0 stable is declined by the chosen-order execution"
    );
    assert!(game.pending().is_none());
    game.validate().expect("state stays valid");
}

#[test]
fn a037_remains_unimplemented_and_does_not_alter_anything() {
    let set = verified_basic_card_set(2).expect("verified 2-player set");
    let card = set.definition("A037").expect("A037 is registered");
    assert_eq!(card.kind, CardKind::MinorImprovement);
    assert_eq!(card.cost, vec![(Good::Wood, 1)]);
    assert_eq!(card.points, 0);
    // The exchange effect (pay wood -> gain grain + point) is deliberately not
    // implemented yet: recorded as needs_resolution in the PR body.
    assert!(card.effects.is_empty(), "A037's effects stay empty until resolved");
}
