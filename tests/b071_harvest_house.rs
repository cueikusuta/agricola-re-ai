//! Tests for B071 収穫小屋 (Harvest House): conditional on-play gain bundle.
//!
//! B071 is a minor improvement costing wood 1 + clay 1 + reed 1 and printing
//! 2 VP. On play, when the number of completed harvests
//! (`HARVEST_ROUNDS` strictly before the current round) equals the number of
//! occupations the owner has played, the owner gains 1 food, 1 grain and
//! 1 vegetable. The bundle is three gains, so the definition carries three
//! `CardEffect::OnPlayGainIfHarvestsEqualOccupations` effects (one per good).
//!
//! Harvests skip rounds (`HARVEST_ROUNDS = [4, 7, 9, 11, 13, 14]`), so the
//! completed-harvest count is NOT `round - 1`: in round 8 the completed
//! harvests are rounds 4 and 7 (count 2), in round 11 rounds 4/7/9 (count 3).
//!
//! Every behavioural case goes through the real dispatcher
//! (`card_events::dispatch_play`) with an explicit round, because the generic
//! `cards::apply_on_play_effects` catch-all no-ops this variant (the handler
//! needs the round and the card set). Covers registry reachability, the
//! printed definition shape, the B071-normal / B071-not-triggered spec
//! contracts, the multiple-completed-harvests boundaries, the zero equality
//! case, the occupation count excluding this minor improvement itself, and
//! the no-double-grant guarantee across an unrelated follow-up play.

use agricola_re_ai::card_events::dispatch_play;
use agricola_re_ai::cards::{
    verified_band_b_cards, CardDefinition, CardEffect, CardKind, CardSet, PostPlayDestination,
};
use agricola_re_ai::model::{Good, Player};
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

/// The printed bundle, in the definition's effect order.
const PRINTED_GOODS: [Good; 3] = [Good::Food, Good::Grain, Good::Vegetable];

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

fn b071_card() -> CardDefinition {
    verified_band_b_cards()
        .into_iter()
        .find(|card| card.id == "B071")
        .expect("B071 must be defined in verified_band_b_cards")
}

fn b071_set() -> CardSet {
    CardSet {
        definitions: vec![
            b071_card(),
            occ_def("occ1"),
            occ_def("occ2"),
            occ_def("occ3"),
            occ_def("occ4"),
        ],
    }
}

/// Play `card_id` for player 0 in `round` through the real dispatcher.
///
/// The caller-side transaction mirrors production: fund the printed cost,
/// pay it, move the new card into the played area, then dispatch. Cards
/// already listed in `played_before` stay in the played area, so the
/// occupation count seen by the handler includes them.
fn play_card(set: &CardSet, players: &mut [Player], card_id: &str, round: u8) {
    let card = set
        .definition(card_id)
        .unwrap_or_else(|| panic!("{card_id} missing from test set"));
    for (good, amount) in &card.cost {
        players[0].supply.set(*good, *amount);
        players[0]
            .supply
            .pay(*good, *amount)
            .unwrap_or_else(|error| panic!("pay {card_id} cost: {error}"));
    }
    players[0].played_cards.push(card_id.to_owned());

    let mut cards = agricola_re_ai::cards::CardState {
        hands: vec![vec![card_id.to_owned()], vec![]],
        ..Default::default()
    };
    let mut market = vec![];
    let mut well = BTreeMap::new();
    let mut budget = 64u32;
    let mut uses = BTreeMap::new();
    let limits = BTreeMap::new();
    let mut pending = None;
    dispatch_play(
        set,
        0,
        card_id,
        0,
        &mut cards,
        players,
        &mut market,
        &mut well,
        round,
        &BTreeSet::new(),
        &mut budget,
        &mut uses,
        &limits,
        &mut pending,
        true,
    )
    .unwrap_or_else(|error| panic!("dispatch_play {card_id}: {error}"));
    assert!(
        pending.is_none(),
        "{card_id} must not create a pending decision"
    );
}

/// Play B071 for player 0 in `round`, with the listed cards already played.
fn play_b071(played_before: &[&str], round: u8) -> Player {
    let set = b071_set();
    let mut players = vec![Player::new(0, 0), Player::new(1, 0)];
    for id in played_before {
        players[0].played_cards.push((*id).to_owned());
    }
    play_card(&set, &mut players, "B071", round);
    players[0].clone()
}

fn assert_only(player: &Player, granted: &[(Good, u32)]) {
    for good in ALL_GOODS {
        let expected = granted
            .iter()
            .find(|(candidate, _)| *candidate == good)
            .map_or(0, |(_, amount)| *amount);
        assert_eq!(
            player.supply.get(good),
            expected,
            "wrong amount for {good:?} (expected {expected})"
        );
    }
}

#[test]
fn b071_reachable_via_default_registry() {
    let registry = build_default_registry().expect("default registry builds");
    let card_id = registry.card_id("B071").expect("B071 must have a CardId");
    let card = registry.get(card_id).expect("B071 must be retrievable");
    assert_eq!(card.external_id, "B071");
    assert_eq!(card.kind, CardKind::MinorImprovement);
    assert!(card.effects.iter().any(|effect| matches!(
        effect,
        CardEffect::OnPlayGainIfHarvestsEqualOccupations { .. }
    )));
}

#[test]
fn b071_band_definition_matches_printed_shape() {
    let card = b071_card();
    assert_eq!(card.kind, CardKind::MinorImprovement);
    assert_eq!(
        card.cost,
        vec![(Good::Wood, 1), (Good::Clay, 1), (Good::Reed, 1)]
    );
    assert_eq!(card.points, 2);
    assert_eq!(card.post_play, PostPlayDestination::PlayedArea);

    let grants: Vec<(Good, u32)> = card
        .effects
        .iter()
        .filter_map(|effect| match effect {
            CardEffect::OnPlayGainIfHarvestsEqualOccupations { good, amount } => {
                assert_eq!(
                    effect.public_tag(),
                    "on_play_gain_if_harvests_equal_occupations"
                );
                Some((*good, *amount))
            }
            _ => None,
        })
        .collect();
    assert_eq!(
        grants,
        vec![(Good::Food, 1), (Good::Grain, 1), (Good::Vegetable, 1),],
        "B071 must carry exactly three one-of-each gains"
    );
}

/// B071-normal (spec contract): 2 completed harvests, 2 occupations played.
/// Round 8 is the first round after both harvest rounds 4 and 7.
#[test]
fn b071_normal_round_8_two_occupations_gains_bundle() {
    let player = play_b071(&["occ1", "occ2"], 8);
    assert_only(
        &player,
        &[(Good::Food, 1), (Good::Grain, 1), (Good::Vegetable, 1)],
    );
}

/// B071-not-triggered (spec contract): 2 completed harvests, 1 occupation.
#[test]
fn b071_not_triggered_round_8_one_occupation_gains_nothing() {
    let player = play_b071(&["occ1"], 8);
    assert_only(&player, &[]);
}

/// Boundary across multiple completed harvests: round 11 has harvests 4/7/9
/// behind it (count 3), so three occupations trigger the bundle there too.
#[test]
fn b071_boundary_round_11_three_occupations_gains_bundle() {
    let player = play_b071(&["occ1", "occ2", "occ3"], 11);
    assert_only(
        &player,
        &[(Good::Food, 1), (Good::Grain, 1), (Good::Vegetable, 1)],
    );
}

/// Same boundary from the other side: in round 9 only rounds 4 and 7 are
/// completed (count 2), so three occupations must NOT trigger.
#[test]
fn b071_boundary_round_9_three_occupations_gains_nothing() {
    let player = play_b071(&["occ1", "occ2", "occ3"], 9);
    assert_only(&player, &[]);
}

/// Zero equality: before the first harvest (round 4) and with no occupations
/// played, 0 == 0 holds, so the bundle is granted.
#[test]
fn b071_zero_equality_round_4_no_occupations_gains_bundle() {
    let player = play_b071(&[], 4);
    assert_only(
        &player,
        &[(Good::Food, 1), (Good::Grain, 1), (Good::Vegetable, 1)],
    );
}

/// The occupation count must exclude this minor improvement itself: with only
/// B071 played at round 8 (2 completed harvests) the count is 0, so nothing
/// is granted. The normal case above proves the played minors do not inflate
/// the count to 3 either.
#[test]
fn b071_minor_improvement_is_not_an_occupation() {
    let set = b071_set();
    let mut players = vec![Player::new(0, 0), Player::new(1, 0)];
    play_card(&set, &mut players, "B071", 8);
    assert_only(&players[0], &[]);
}

/// No double grant: after the normal play grants the bundle once, playing an
/// unrelated effect-less occupation in the same round grants nothing more,
/// and every good of the bundle stays at exactly 1.
#[test]
fn b071_unrelated_follow_up_play_does_not_regrant() {
    let set = b071_set();
    let mut players = vec![Player::new(0, 0), Player::new(1, 0)];
    players[0].played_cards = vec!["occ1".to_owned(), "occ2".to_owned()];
    play_card(&set, &mut players, "B071", 8);
    assert_only(
        &players[0],
        &[(Good::Food, 1), (Good::Grain, 1), (Good::Vegetable, 1)],
    );

    play_card(&set, &mut players, "occ4", 8);
    assert_only(
        &players[0],
        &[(Good::Food, 1), (Good::Grain, 1), (Good::Vegetable, 1)],
    );
}

/// The definition exposes exactly the three printed goods through the enum,
/// one effect per good, so a single play grants each good at most once.
#[test]
fn b071_single_play_grants_each_good_at_most_once() {
    let card = b071_card();
    for good in PRINTED_GOODS {
        let hits = card
            .effects
            .iter()
            .filter(|effect| {
                matches!(
                    effect,
                    CardEffect::OnPlayGainIfHarvestsEqualOccupations {
                        good: effect_good,
                        amount: 1,
                    } if *effect_good == good
                )
            })
            .count();
        assert_eq!(
            hits, 1,
            "{good:?} must appear exactly once in the definition"
        );
    }
}
