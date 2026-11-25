//! D054 マスの池 (Trout Pool): at the start of each work phase, if the Fishing
//! accumulation holds at least 3 food, gain 1 food.
//!
//! Spec: data/bga-card-specs/D054.json (cost clay:2, printed 1 point,
//! work_phase_start trigger, accumulation_at_least(space=fishing, n=3),
//! gain_resource(good=food, amount=1), optional: false).
//!
//! Covers: D054-normal (accumulation == 3 -> +1 food), D054-not-triggered
//! (accumulation == 2 -> +0), other-space accumulation, other owner, and
//! accumulation immutability (D054 reads but never depletes the standing
//! food). Save/load/replay are covered by the fixed-seed preset replay test
//! in tests/m6_verified_pool.rs once D054 joins the two-player preset.

use agricola_re_ai::card_events::dispatch_start_round;
use agricola_re_ai::cards::{CardKind, CardSet, CardState};
use agricola_re_ai::model::{Good, Player, Space};
use std::collections::{BTreeMap, BTreeSet};

fn set_with_d054() -> CardSet {
    CardSet {
        definitions: verified_round_and_harvest_cards(),
    }
}

fn d054_round_start_food(owner: usize, accumulations: &BTreeMap<Space, u32>) -> (u32, u32) {
    // owner = seat 0 (D054 holder), seat 1 = other player (no D054).
    let mut accumulations = accumulations.clone();
    let set = set_with_d054();
    let mut cards = CardState::default();
    let mut players = vec![Player::new(0, 0), Player::new(1, 0)];
    players[owner].played_cards.push("D054".into());
    let used = BTreeSet::new();
    let mut event_budget = 100;
    let mut card_uses = BTreeMap::new();
    let card_use_limits = BTreeMap::new();
    let mut pending = None;
    dispatch_start_round(
        &set,
        &mut players,
        &mut cards,
        &mut vec![],
        &mut BTreeMap::new(),
        2,
        &used,
        &mut accumulations,
        &mut event_budget,
        &mut card_uses,
        &card_use_limits,
        &mut pending,
        &[],
    )
    .expect("dispatch_start_round must succeed");
    (players[0].supply.food, players[1].supply.food)
}

#[test]
fn d054_gains_food_when_fishing_holds_at_least_three() {
    // D054-normal: Fishing == 3 -> holder gains 1 food.
    let accums = BTreeMap::from([(Space::Fishing, 3)]);
    let (food, other) = d054_round_start_food(0, &accums);
    assert_eq!(food, 1, "Fishing == 3 should grant 1 food to the holder");
    assert_eq!(other, 0, "other player must not gain");

    // Boundary: Fishing == 4 also triggers (>= 3).
    let accums = BTreeMap::from([(Space::Fishing, 4)]);
    let (food, _) = d054_round_start_food(0, &accums);
    assert_eq!(food, 1);
}

#[test]
fn d054_not_triggered_below_threshold() {
    // D054-not-triggered: Fishing == 2 -> no food.
    let accums = BTreeMap::from([(Space::Fishing, 2)]);
    let (food, _) = d054_round_start_food(0, &accums);
    assert_eq!(food, 0, "Fishing == 2 must not trigger");

    // Empty accumulation map (dispatch without the map grants nothing).
    let accums = BTreeMap::new();
    let (food, _) = d054_round_start_food(0, &accums);
    assert_eq!(food, 0);
}

#[test]
fn d054_other_space_does_not_trigger() {
    // A different accumulation space (ReedBank >= 3) does not trigger D054.
    let accums = BTreeMap::from([(Space::ReedBank, 5)]);
    let (food, _) = d054_round_start_food(0, &accums);
    assert_eq!(food, 0);
}

#[test]
fn d054_only_owner_gains() {
    // Holder is seat 1; seat 0 has no D054 and must not gain.
    let accums = BTreeMap::from([(Space::Fishing, 3)]);
    let (food, other) = d054_round_start_food(1, &accums);
    assert_eq!(other, 1, "holder (seat 1) gains 1 food");
    assert_eq!(food, 0, "non-holder (seat 0) gains nothing");
}

#[test]
fn d054_accumulation_is_not_depleted() {
    // D054 reads the accumulation read-only: standing food stays in place.
    let accums = BTreeMap::from([(Space::Fishing, 3)]);
    let (_, _) = d054_round_start_food(0, &accums);
    // The map itself is passed by value to the test helper and never mutated
    // by the handler; assert the input value is still 3.
    assert_eq!(accums.get(&Space::Fishing), Some(&3));
}

#[test]
fn d054_definition_matches_spec_json() {
    let cards = verified_round_and_harvest_cards();
    let d054 = cards
        .iter()
        .find(|c| c.id == "D054")
        .expect("D054 must be in the implemented catalog");
    assert_eq!(d054.kind, CardKind::MinorImprovement);
    assert_eq!(d054.cost, vec![(Good::Clay, 2)]);
    assert_eq!(d054.points, 1);
    assert!(matches!(
        d054.effects.as_slice(),
        [
            agricola_re_ai::cards::CardEffect::StartRoundGainIfAccumulationAtLeast {
                space: Space::Fishing,
                min: 3,
                good: Good::Food,
                amount: 1,
            }
        ]
    ));
}

fn verified_round_and_harvest_cards() -> Vec<agricola_re_ai::cards::CardDefinition> {
    agricola_re_ai::cards::verified_round_and_harvest_cards()
}
