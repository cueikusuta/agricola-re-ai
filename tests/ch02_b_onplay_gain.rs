//! Tests for ch02 第2章 cards (B054/B117/B151/B160) added by the
//! restack session 2026-11-23 (B-deck on-play simple gains, chapter 1).
//!
//! Every card below prints exactly one constant `gain_resource` step on its
//! `on_play` trigger, modelled with the existing `CardEffect::OnPlayGain`
//! variant (no new Effect variant / EventKind / saved field). Their remaining
//! printed effects live on other triggers and stay unwired here; the spec
//! `implementation_status` is `in_progress` and the owning follow-up is noted
//! in `src/cards.rs` and the PR body.
//!
//! Sources (third-party BGA wiki card pages recorded in each spec JSON, checked
//! 2026-11-09 / 2026-11-14; NOT official Lookout text):
//! B054.json (p=3203), B117.json (p=3140), B151.json
//! (p=3106), B160.json (p=3097).
//!
//! Covers, per card: registry reachability, definition/spec parity, the
//! effect-only path (`cards::apply_on_play_effects`), cost and player-count
//! boundaries (`cards::can_play`, `CardSet::validate`, pool projection), the
//! dispatcher route (`card_events::dispatch_play`), and the no-fire guarantee
//! (playing one card never triggers another card's on-play gain).

use agricola_re_ai::card_events::dispatch_play;
use agricola_re_ai::cards::{
    apply_on_play_effects, can_play, card_set_for_game, verified_b_onplay_simple_cards_extended,
    CardDefinition, CardEffect, CardKind, CardRequirement, CardSet,
};
use agricola_re_ai::model::{Good, Player};
use agricola_re_ai::registry::{build_default_registry, CardPool};
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

/// One chapter card: id, printed cost, on-play gain. Parity with the spec JSONs.
#[derive(Clone, Copy)]
struct ChapterCard {
    id: &'static str,
    cost: &'static [(Good, u32)],
    good: Good,
    amount: u32,
}

const CHAPTER_CARDS: [ChapterCard; 4] = [
    ChapterCard {
        id: "B054",
        cost: &[(Good::Wood, 1)],
        good: Good::Food,
        amount: 2,
    },
    ChapterCard {
        id: "B117",
        cost: &[],
        good: Good::Wood,
        amount: 1,
    },
    ChapterCard {
        id: "B151",
        cost: &[],
        good: Good::Stone,
        amount: 1,
    },
    ChapterCard {
        id: "B160",
        cost: &[],
        good: Good::Grain,
        amount: 1,
    },
];

fn chapter_card(id: &str) -> CardDefinition {
    verified_b_onplay_simple_cards_extended()
        .into_iter()
        .find(|card| card.id == id)
        .unwrap_or_else(|| {
            panic!("{id} must be defined in verified_b_onplay_simple_cards_extended")
        })
}

/// CardSet holding every chapter card plus filler ids, so prerequisite and
/// played-card bookkeeping have something to reference.
fn chapter_set() -> CardSet {
    let mut definitions: Vec<CardDefinition> = CHAPTER_CARDS
        .iter()
        .map(|entry| chapter_card(entry.id))
        .collect();
    definitions.push(CardDefinition {
        use_limit: None,
        id: "OCC_FILLER".to_owned(),
        kind: CardKind::Occupation,
        cost: vec![],
        points: 0,
        effects: vec![],
        // filler card stays in the played area
        post_play: agricola_re_ai::cards::PostPlayDestination::PlayedArea,
    });
    CardSet { definitions }
}

fn player_with(good: Good, amount: u32) -> Player {
    let mut player = Player::new(0, 0);
    match good {
        Good::Wood => player.supply.wood = amount,
        Good::Clay => player.supply.clay = amount,
        Good::Reed => player.supply.reed = amount,
        Good::Stone => player.supply.stone = amount,
        Good::Food => player.supply.food = amount,
        Good::Grain => player.supply.grain = amount,
        Good::Vegetable => player.supply.vegetable = amount,
    }
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
            "no chapter gain may fire here, but {candidate:?} changed"
        );
    }
}

/// Pay the printed cost (the test stands in for the play action's payment),
/// then run the card through the real play dispatcher. Mirrors the production
/// order: the card has entered the played area before dispatch_play runs.
fn dispatch_chapter_card(set: &CardSet, player: &mut Player, id: &str) {
    let card = set
        .definition(id)
        .unwrap_or_else(|| panic!("{id} missing from test set"));
    for (good, amount) in &card.cost {
        player
            .supply
            .pay(*good, *amount)
            .unwrap_or_else(|error| panic!("pay {id} cost: {error}"));
    }

    let mut cards = agricola_re_ai::cards::CardState {
        hands: vec![vec![id.to_owned()], vec![]],
        ..Default::default()
    };
    let mut players = vec![player.clone(), Player::new(1, 0)];
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
        true,
    )
    .unwrap_or_else(|error| panic!("dispatch_play {id}: {error}"));
    assert!(pending.is_none(), "{id} must not create a pending decision");
    *player = players[0].clone();
}

#[test]
fn ch02_cards_reachable_via_default_registry() {
    let registry = build_default_registry().expect("default registry builds");
    for entry in CHAPTER_CARDS {
        let id = entry.id;
        let card_id = registry
            .card_id(id)
            .unwrap_or_else(|| panic!("{id} must have a CardId"));
        let card = registry
            .get(card_id)
            .unwrap_or_else(|| panic!("{id} must be retrievable by CardId"));
        assert_eq!(card.external_id, id, "round-trip external_id for {id}");
        let expected_kind = match id {
            "B054" => CardKind::MinorImprovement,
            _ => CardKind::Occupation,
        };
        assert_eq!(card.kind, expected_kind, "{id} kind must match the spec");
        assert!(
            card.effects
                .iter()
                .any(|effect| matches!(effect, CardEffect::OnPlayGain { .. })),
            "{id} must expose the existing OnPlayGain variant"
        );
    }
}

#[test]
fn ch02_definitions_match_spec_parity() {
    for entry in CHAPTER_CARDS {
        let (id, cost, good, amount) = (entry.id, entry.cost, entry.good, entry.amount);
        let card = chapter_card(id);
        assert_eq!(card.id, id);
        assert_eq!(card.points, 0, "{id} prints 0 VP");
        assert_eq!(&card.cost[..], cost, "{id} cost must match the spec");
        assert_eq!(
            card.use_limit, None,
            "{id} has no printed per-round use limit"
        );

        let gains: Vec<(Good, u32)> = card
            .effects
            .iter()
            .filter_map(|effect| match effect {
                CardEffect::OnPlayGain {
                    good: gain_good,
                    amount: gain_amount,
                } => Some((*gain_good, *gain_amount)),
                _ => None,
            })
            .collect();
        assert_eq!(
            gains,
            vec![(good, amount)],
            "{id} must carry exactly its printed on-play gain"
        );
        assert_eq!(
            card.effects
                .iter()
                .filter(|effect| matches!(effect, CardEffect::OnPlayGain { .. }))
                .count(),
            1,
            "{id} must not double-grant its on-play gain"
        );

        // No new vocabulary: the only non-gain effect allowed is the 4-player
        // eligibility guard for the 148-168 band cards.
        for effect in &card.effects {
            match effect {
                CardEffect::OnPlayGain { .. } => {}
                CardEffect::Prerequisite {
                    rule: CardRequirement::MinimumPlayers(4),
                } => assert!(
                    matches!(id, "B151" | "B160"),
                    "{id} must not carry a 4-player guard (148-168 rule only)"
                ),
                other => panic!("{id} carries unexpected effect {other:?}"),
            }
        }
    }
}

#[test]
fn b054_grants_exactly_two_food_on_play() {
    let set = chapter_set();
    let mut player = Player::new(0, 0);
    apply_on_play_effects(&set, &mut player, "B054").expect("apply B054");
    assert_only_good(&player, Good::Food, 2);
}

#[test]
fn b117_grants_one_wood_on_play() {
    let set = chapter_set();
    let mut player = Player::new(0, 0);
    apply_on_play_effects(&set, &mut player, "B117").expect("apply B117");
    assert_only_good(&player, Good::Wood, 1);
}

#[test]
fn b151_grants_one_stone_on_play() {
    let set = chapter_set();
    let mut player = Player::new(0, 0);
    apply_on_play_effects(&set, &mut player, "B151").expect("apply B151");
    assert_only_good(&player, Good::Stone, 1);
}

#[test]
fn b160_grants_one_grain_on_play() {
    let set = chapter_set();
    let mut player = Player::new(0, 0);
    apply_on_play_effects(&set, &mut player, "B160").expect("apply B160");
    assert_only_good(&player, Good::Grain, 1);
}

#[test]
fn b054_cost_boundary_requires_the_printed_wood() {
    let set = chapter_set();
    let card = chapter_card("B054");
    let mut player = player_with(Good::Wood, 1);
    assert!(
        can_play(&set, &card, &player, 0),
        "exactly 1 wood must make B054 playable"
    );
    player.supply.wood = 0;
    assert!(
        !can_play(&set, &card, &player, 0),
        "the printed 1-wood cost is still required"
    );
}

#[test]
fn b117_occupation_is_free_to_play() {
    let set = chapter_set();
    let card = chapter_card("B117");
    let player = Player::new(0, 0);
    assert!(
        can_play(&set, &card, &player, 0),
        "occupations carry no printed cost, so B117 must be playable with empty supply"
    );
}

/// Raw set padded to the 7-per-kind-per-player deal minimum so the only
/// remaining variable is the player-count guard itself.
fn padded_raw_set(card_id: &str, players: usize) -> CardSet {
    let mut definitions = vec![chapter_card(card_id)];
    let per_kind = 7 * players;
    for kind in [CardKind::Occupation, CardKind::MinorImprovement] {
        for index in 0..per_kind {
            definitions.push(CardDefinition {
                use_limit: None,
                id: format!("FILLER_{}_{index}", kind as u8),
                kind,
                cost: vec![],
                points: 0,
                effects: vec![],
                // filler card stays in the played area
                post_play: agricola_re_ai::cards::PostPlayDestination::PlayedArea,
            });
        }
    }
    CardSet { definitions }
}

/// The 148-168 band cards are 4-player only: the MinimumPlayers(4) guard must
/// reject raw 2/3-player sets and must project the gain only at 4 players.
#[test]
fn b151_and_b160_player_count_guard_boundary() {
    for id in ["B151", "B160"] {
        assert!(
            padded_raw_set(id, 4).validate(4).is_ok(),
            "{id} must validate at 4 players"
        );
        for players in [2usize, 3] {
            assert!(
                padded_raw_set(id, players).validate(players).is_err(),
                "{id} must not be valid in a raw {players}-player set (148-168 rule)"
            );
        }

        let registry = build_default_registry().expect("default registry builds");
        let card_id = registry
            .card_id(id)
            .unwrap_or_else(|| panic!("{id} must have a CardId"));
        for (players, expected) in [(2usize, false), (3, false), (4, true)] {
            let pool = CardPool::new(&registry, players, true);
            assert_eq!(
                pool.contains(card_id),
                expected,
                "{id} experimental-pool eligibility at {players} players"
            );
        }
    }
}

/// Pool projection across player counts: the 1-84 / 85-126 cards enter every
/// count; the 148-168 cards only at 4.
#[test]
fn ch02_pool_projection_by_player_count() {
    for (players, expected_ids) in [
        (2usize, &["B054", "B117"][..]),
        (3, &["B054", "B117"][..]),
        (4, &["B054", "B117", "B151", "B160"][..]),
    ] {
        let set = card_set_for_game(players, true);
        for id in expected_ids {
            assert!(
                set.definitions.iter().any(|card| card.id == *id),
                "{players}-player experimental set must contain {id}"
            );
        }
        if players < 4 {
            for id in ["B151", "B160"] {
                assert!(
                    !set.definitions.iter().any(|card| card.id == *id),
                    "{id} must stay out of the {players}-player set"
                );
            }
        }
    }
}

/// No-fire guarantee: playing one chapter card grants only its own gain, and
/// an unknown / unplayed card grants nothing.
#[test]
fn exactly_one_chapter_card_fires_per_play() {
    let set = chapter_set();
    for entry in CHAPTER_CARDS {
        let (id, good, amount) = (entry.id, entry.good, entry.amount);
        let mut player = Player::new(0, 0);
        apply_on_play_effects(&set, &mut player, id)
            .unwrap_or_else(|error| panic!("apply {id}: {error}"));
        assert_only_good(&player, good, amount);
    }

    let mut untouched = Player::new(0, 0);
    assert!(
        apply_on_play_effects(&set, &mut untouched, "B999").is_err(),
        "an unknown card id must be rejected"
    );
    assert_no_goods(&untouched);
}

#[test]
fn ch02_dispatcher_route_grants_the_printed_gain() {
    let set = chapter_set();
    for entry in CHAPTER_CARDS {
        let (id, good, amount) = (entry.id, entry.good, entry.amount);
        let mut player = Player::new(0, 0);
        // Give the player the printed cost where there is one.
        if let Some((cost_good, cost_amount)) = chapter_card(id).cost.first().copied() {
            player = player_with(cost_good, cost_amount);
        }
        dispatch_chapter_card(&set, &mut player, id);
        assert_only_good(&player, good, amount);
    }
}

#[test]
fn ch02_dispatcher_route_does_not_fire_other_cards() {
    let set = chapter_set();
    let mut player = player_with(Good::Wood, 1); // B054's printed cost
                                                 // Only B054 is dispatched; the other four on-play gains must stay silent.
    dispatch_chapter_card(&set, &mut player, "B054");
    assert_only_good(&player, Good::Food, 2);
}
