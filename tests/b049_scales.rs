//! Tests for B049 天秤 (Scales): after-placement gain when improvements
//! equal occupations.
//!
//! B049 costs 1 wood, prints 0 VP, is a Minor Improvement with
//! `Prerequisite{MaximumOccupations(0)}`. Each time the owner places an
//! occupation or an improvement, if the number of improvements in front
//! (base improvements + played minor improvements) equals the number of
//! occupations *after* the zone move, gain 2 food (snapshot=after_event).
//! The effect fires from `dispatch_play`'s two dedicated after-play scans
//! (Phase 4 for occupations, Phase 4b for improvements) and never from the
//! generic phase scan or the legacy `cards::play`/`apply_on_play_effects`
//! path.
//!
//! Counts include the newly placed card *and* B049 itself once it is in
//! the played area, so:
//! - {B049 played} + play occupation → 1 improvement vs 1 occupation → +2 food
//! - {B049 played} + play occupation when 1 occupation already present
//!   → 1 vs 2 → +0
//! - {B049 + 2 occupations played} + play improvement → 2 vs 2 → +2 food
//! - {B049 + 1 occupation played} + play improvement → 2 vs 1 → +0

use agricola_re_ai::card_events::dispatch_play;
use agricola_re_ai::cards::{
    apply_on_play_effects, can_play, verified_band_b_cards, CardDefinition, CardEffect, CardKind,
    CardRequirement, CardSet,
};
use agricola_re_ai::model::{Good, Player};
use agricola_re_ai::registry::build_default_registry;
use std::collections::{BTreeMap, BTreeSet};

fn b049_card() -> CardDefinition {
    verified_band_b_cards()
        .into_iter()
        .find(|card| card.id == "B049")
        .expect("B049 must be defined in verified_band_b_cards")
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

fn b049_set() -> CardSet {
    CardSet {
        definitions: vec![
            b049_card(),
            occ_def("occ1"),
            occ_def("occ2"),
            minor_def("imp1"),
        ],
    }
}

fn b049_player(played: &[&str]) -> Player {
    let mut player = Player::new(0, 0);
    player.played_cards = played.iter().map(|id| (*id).to_owned()).collect();
    player
}

fn assert_food_only(player: &Player, food: u32) {
    for candidate in [
        Good::Wood,
        Good::Clay,
        Good::Reed,
        Good::Stone,
        Good::Grain,
        Good::Vegetable,
    ] {
        assert_eq!(
            player.supply.get(candidate),
            0,
            "unexpected {candidate:?} when expecting only food x{food}"
        );
    }
    assert_eq!(player.supply.get(Good::Food), food, "food amount mismatch");
}

/// Dispatch `new_card_id` for seat 0 through the production dispatcher. The
/// caller-side zone move (new card last in `played_cards`) matches `cards.rs`
/// `play`: `dispatch_play` runs after the card has entered the played area.
fn dispatch_card(
    set: &CardSet,
    player: &mut Player,
    new_card_id: &str,
    is_improvement_action: bool,
) {
    let card = set
        .definition(new_card_id)
        .expect("{new_card_id} missing from test set");
    for (good, amount) in &card.cost {
        player
            .supply
            .pay(*good, *amount)
            .unwrap_or_else(|_| panic!("pay {new_card_id} cost: {good:?} x{amount}"));
    }

    let mut cards = agricola_re_ai::cards::CardState {
        hands: vec![vec![new_card_id.to_owned()], vec![]],
        ..Default::default()
    };
    let mut players = vec![player.clone(), Player::new(1, 0)];
    players[0].played_cards.push(new_card_id.to_owned());
    let mut market = vec![];
    let mut well = BTreeMap::new();
    let mut budget = 64u32;
    let mut uses = BTreeMap::new();
    let limits = BTreeMap::new();
    let mut pending = None;
    dispatch_play(
        set,
        0,
        new_card_id,
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
        is_improvement_action,
    )
    .unwrap_or_else(|error| panic!("dispatch_play {new_card_id}: {error}"));
    assert!(
        pending.is_none(),
        "{new_card_id} must not create a pending decision (no resume/replay grant)"
    );
    *player = players[0].clone();
}

#[test]
fn b049_reachable_via_default_registry() {
    let registry = build_default_registry().expect("default registry builds");
    let card_id = registry.card_id("B049").expect("B049 must have a CardId");
    let card = registry.get(card_id).expect("B049 must be retrievable");
    assert_eq!(card.external_id, "B049");
    assert_eq!(card.kind, CardKind::MinorImprovement);
    assert!(card.effects.iter().any(|effect| matches!(
        effect,
        CardEffect::AfterPlacementGainIfImprovementsEqualOccupations { .. }
    )));
}

#[test]
fn b049_band_definition_matches_spec() {
    let card = b049_card();
    assert_eq!(card.kind, CardKind::MinorImprovement);
    assert_eq!(card.cost, vec![(Good::Wood, 1)]);
    assert_eq!(card.points, 0);
    assert!(matches!(
        card.effects.first(),
        Some(CardEffect::Prerequisite {
            rule: CardRequirement::MaximumOccupations(0)
        })
    ));
    assert!(matches!(
        card.effects.last(),
        Some(
            CardEffect::AfterPlacementGainIfImprovementsEqualOccupations {
                good: Good::Food,
                amount: 2,
            }
        )
    ));
    assert_eq!(
        card.effects.last().map(|e| e.public_tag()),
        Some("after_placement_gain_if_improvements_equal_occupations")
    );
}

#[test]
fn b049_prereq_gating() {
    let set = b049_set();
    let card = set.definition("B049").expect("B049 missing");
    let mut player = b049_player(&[]);
    player.supply.set(Good::Wood, 1);
    assert!(
        can_play(&set, card, &player, 0),
        "0 occupations satisfies MaximumOccupations(0)"
    );
    player.played_cards.push("occ1".to_owned());
    assert!(
        !can_play(&set, card, &player, 0),
        "1 occupation violates MaximumOccupations(0)"
    );
    let mut poor = b049_player(&[]);
    poor.supply.set(Good::Wood, 0);
    assert!(!can_play(&set, card, &poor, 0), "1 wood cost is required");
}

#[test]
fn b049_own_play_does_not_fire() {
    // B049's own placement is not an occupation/improvement placement *after*
    // this card; no grant at B049 play time.
    let set = b049_set();
    let mut player = b049_player(&[]);
    player.supply.set(Good::Wood, 1); // B049 costs 1 wood, paid by the caller
    dispatch_card(&set, &mut player, "B049", false);
    assert_food_only(&player, 0);
}

#[test]
fn b049_dispatcher_occupation_equal_grants_food_2() {
    // Phase 4 occupation scan: {B049} + play occ1 → 1 improvement vs 1 occupation.
    let set = b049_set();
    let mut player = b049_player(&["B049"]);
    dispatch_card(&set, &mut player, "occ1", false);
    assert_food_only(&player, 2);
}

#[test]
fn b049_dispatcher_occupation_unequal_grants_nothing() {
    let set = b049_set();
    let mut player = b049_player(&["B049", "occ1"]);
    dispatch_card(&set, &mut player, "occ2", false);
    assert_food_only(&player, 0);
}

#[test]
fn b049_dispatcher_improvement_equal_grants_food_2() {
    // Phase 4b improvement scan: {B049, occ1, occ2} + play imp1 → 2 vs 2.
    let set = b049_set();
    let mut player = b049_player(&["B049", "occ1", "occ2"]);
    dispatch_card(&set, &mut player, "imp1", true);
    assert_food_only(&player, 2);
}

#[test]
fn b049_dispatcher_improvement_unequal_grants_nothing() {
    let set = b049_set();
    let mut player = b049_player(&["B049", "occ1"]);
    dispatch_card(&set, &mut player, "imp1", true);
    assert_food_only(&player, 0);
}

#[test]
fn b049_no_regrant_on_subsequent_unrelated_action() {
    let set = b049_set();
    let mut player = b049_player(&["B049"]);
    dispatch_card(&set, &mut player, "occ1", false);
    assert_food_only(&player, 2); // grant once
                                  // A subsequent unrelated occupation keeps the counts unequal: no re-grant,
                                  // and the earlier grant is not touched.
    dispatch_card(&set, &mut player, "occ2", false);
    assert_food_only(&player, 2);
}

#[test]
fn b049_legacy_path_negative_control() {
    // Counts are equal (B049 + occ1 → 1 vs 1) but the legacy apply path must
    // not re-scan after-play triggers.
    let set = b049_set();
    let mut player = b049_player(&["B049", "occ1"]);
    apply_on_play_effects(&set, &mut player, "B049").expect("apply B049 via legacy path");
    assert_food_only(&player, 0);
}
