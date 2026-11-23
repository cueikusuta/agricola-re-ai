//! Tests for B073 贈り物かご (Gift Basket): on-play room-count gain table.
//!
//! B073 costs 1 reed and prints 1 VP. On play, when the owner's room count is
//! exactly 2/3/4/5, gain 1 vegetable/food/grain/vegetable respectively; any
//! other room count grants nothing.
//!
//! The printed table is modelled as four `CardEffect::OnPlayGainIfRoomCount`
//! rows. Only the row whose room count matches the owner exactly resolves, so
//! one play grants at most one good (no double grant).
//!
//! Covers registry reachability, vocabulary shape, the effect-only path
//! (`cards::apply_on_play_effects`), prerequisite/cost gating, and the
//! dispatcher route (`card_events::dispatch_play`). The B073-normal, B073-4,
//! and B073-not-triggered spec contracts are exercised through the real paths,
//! together with the 2/5 boundaries and the at-most-one-good guarantee.

use agricola_re_ai::card_events::dispatch_play;
use agricola_re_ai::cards::{
    apply_on_play_effects, can_play, verified_band_b_cards, CardDefinition, CardEffect, CardKind,
    CardRequirement, CardSet,
};
use agricola_re_ai::model::{Good, Player, Tile};
use agricola_re_ai::registry::build_default_registry;
use std::collections::{BTreeMap, BTreeSet};

const ALL_GOODS: [Good; 7] = [
    Good::Wood,
    Good::Clay,
    Good::Reed,
    Good::Stone,
    Good::Food,
    Good::Grain,
    Good::Vegetable,
];

fn occ_def(id: &str) -> CardDefinition {
    CardDefinition {
        use_limit: None,
        id: id.to_owned(),
        kind: CardKind::Occupation,
        cost: vec![],
        points: 0,
        effects: vec![],
        post_play: agricola_re_ai::cards::PostPlayDestination::PlayedArea,
    }
}

fn b073_card() -> CardDefinition {
    verified_band_b_cards()
        .into_iter()
        .find(|card| card.id == "B073")
        .expect("B073 must be defined in verified_band_b_cards")
}

fn b073_set() -> CardSet {
    CardSet {
        definitions: vec![
            b073_card(),
            occ_def("occ1"),
            occ_def("occ2"),
            occ_def("occ3"),
        ],
    }
}

fn player_with_rooms(rooms: usize) -> Player {
    assert!((2..=6).contains(&rooms), "test setup supports 2..=6 rooms");
    let mut player = Player::new(0, 0);
    // Player::new starts with two rooms at farm[0] and farm[5]. Fill cells
    // 1..=4 to reach the requested exact room count without touching farm[5].
    for cell in player.farm.iter_mut().skip(1).take(rooms.saturating_sub(2)) {
        cell.tile = Tile::Room;
    }
    assert_eq!(player.rooms(), rooms, "room setup must be exact");
    player
}

fn assert_only_good(player: &Player, good: Good, amount: u32) {
    for candidate in ALL_GOODS {
        let expected = if candidate == good { amount } else { 0 };
        assert_eq!(
            player.supply.get(candidate),
            expected,
            "expected only {good:?} x{amount}, wrong amount for {candidate:?}"
        );
    }
}

fn assert_no_goods(player: &Player) {
    for candidate in ALL_GOODS {
        assert_eq!(
            player.supply.get(candidate),
            0,
            "non-matching room count must not grant {candidate:?}"
        );
    }
}

fn dispatch_b073(set: &CardSet, player: &mut Player) {
    let card = set.definition("B073").expect("B073 missing from test set");
    for (good, amount) in &card.cost {
        player
            .supply
            .pay(*good, *amount)
            .unwrap_or_else(|error| panic!("pay B073 cost: {error}"));
    }

    let mut cards = agricola_re_ai::cards::CardState {
        hands: vec![vec!["B073".to_owned()], vec![]],
        ..Default::default()
    };
    let mut players = vec![player.clone(), Player::new(1, 0)];
    // Caller-side zone move: dispatch_play runs after the new card has entered
    // the player's played area, matching the production play path.
    players[0].played_cards.push("B073".to_owned());
    let mut market = vec![];
    let mut well = BTreeMap::new();
    let mut budget = 64u32;
    let mut uses = BTreeMap::new();
    let limits = BTreeMap::new();
    let mut pending = None;
    dispatch_play(
        set,
        0,
        "B073",
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
        true,
    )
    .unwrap_or_else(|error| panic!("dispatch_play B073: {error}"));
    assert!(pending.is_none(), "B073 must not create a pending decision");
    *player = players[0].clone();
}

#[test]
fn b073_reachable_via_default_registry() {
    let registry = build_default_registry().expect("default registry builds");
    let card_id = registry.card_id("B073").expect("B073 must have a CardId");
    let card = registry.get(card_id).expect("B073 must be retrievable");
    assert_eq!(card.external_id, "B073");
    assert_eq!(card.kind, CardKind::MinorImprovement);
    assert!(card
        .effects
        .iter()
        .any(|effect| matches!(effect, CardEffect::OnPlayGainIfRoomCount { .. })));
}

#[test]
fn b073_band_definition_matches_printed_table() {
    let card = b073_card();
    assert_eq!(card.kind, CardKind::MinorImprovement);
    assert_eq!(card.cost, vec![(Good::Reed, 1)]);
    assert_eq!(card.points, 1);
    assert!(matches!(
        card.effects.first(),
        Some(CardEffect::Prerequisite {
            rule: CardRequirement::MinimumOccupations(3)
        })
    ));

    let rows: Vec<(u8, Good, u32)> = card
        .effects
        .iter()
        .filter_map(|effect| match effect {
            CardEffect::OnPlayGainIfRoomCount {
                rooms,
                good,
                amount,
            } => {
                assert_eq!(effect.public_tag(), "on_play_gain_if_room_count");
                Some((*rooms, *good, *amount))
            }
            _ => None,
        })
        .collect();
    assert_eq!(
        rows,
        vec![
            (2, Good::Vegetable, 1),
            (3, Good::Food, 1),
            (4, Good::Grain, 1),
            (5, Good::Vegetable, 1),
        ]
    );
}

#[test]
fn b073_normal_rooms_3_gains_one_food() {
    let set = b073_set();
    let mut player = player_with_rooms(3);
    apply_on_play_effects(&set, &mut player, "B073").expect("apply B073");
    assert_only_good(&player, Good::Food, 1);
}

#[test]
fn b073_rooms_4_gains_one_grain() {
    let set = b073_set();
    let mut player = player_with_rooms(4);
    apply_on_play_effects(&set, &mut player, "B073").expect("apply B073");
    assert_only_good(&player, Good::Grain, 1);
}

#[test]
fn b073_not_triggered_rooms_6_gains_nothing() {
    let set = b073_set();
    let mut player = player_with_rooms(6);
    apply_on_play_effects(&set, &mut player, "B073").expect("apply B073");
    assert_no_goods(&player);
}

#[test]
fn b073_boundary_rooms_2_gains_one_vegetable() {
    let set = b073_set();
    let mut player = player_with_rooms(2);
    apply_on_play_effects(&set, &mut player, "B073").expect("apply B073");
    assert_only_good(&player, Good::Vegetable, 1);
}

#[test]
fn b073_boundary_rooms_5_gains_one_vegetable() {
    let set = b073_set();
    let mut player = player_with_rooms(5);
    apply_on_play_effects(&set, &mut player, "B073").expect("apply B073");
    assert_only_good(&player, Good::Vegetable, 1);
}

#[test]
fn b073_requires_three_occupations_and_reed() {
    let set = b073_set();
    let card = set.definition("B073").expect("B073 missing from test set");
    let mut player = player_with_rooms(3);
    player.supply.reed = 1;
    player.played_cards = vec!["occ1".to_owned(), "occ2".to_owned()];
    assert!(
        !can_play(&set, card, &player, 0),
        "2 occupations must not satisfy MinimumOccupations(3)"
    );

    player.played_cards.push("occ3".to_owned());
    assert!(
        can_play(&set, card, &player, 0),
        "3 occupations and 1 reed make B073 playable"
    );

    player.supply.reed = 0;
    assert!(
        !can_play(&set, card, &player, 0),
        "the printed 1-reed cost is still required"
    );
}

#[test]
fn b073_dispatcher_route_rooms_3_gains_one_food() {
    let set = b073_set();
    let mut player = player_with_rooms(3);
    player.supply.reed = 1;
    player.played_cards = vec!["occ1".to_owned(), "occ2".to_owned(), "occ3".to_owned()];
    dispatch_b073(&set, &mut player);
    assert_only_good(&player, Good::Food, 1);
}

#[test]
fn b073_dispatcher_route_rooms_6_gains_nothing() {
    let set = b073_set();
    let mut player = player_with_rooms(6);
    player.supply.reed = 1;
    player.played_cards = vec!["occ1".to_owned(), "occ2".to_owned(), "occ3".to_owned()];
    dispatch_b073(&set, &mut player);
    assert_no_goods(&player);
}
