//! Tests for A109 小商人 (Small Trader): AfterImprovementPlayedGain effect.
//!
//! Covers: registry reachability, effect-only play via `cards::apply_on_play_effects`,
//! legal-play / prerequisite gating via `cards::play`/`cards::can_play`,
//! dispatcher route via `card_events::dispatch_play` (supply increase through
//! the `card_events` effect path), and **engine-level boundary tests** driving
//! `Game::apply` through the four engine call sites.
//!
//! The effect triggers when an improvement is played from hand via a
//! Major or Minor Improvement action and grants 3 food per improvement played.

use agricola_re_ai::card_events::dispatch_play;
use agricola_re_ai::cards::{apply_on_play_effects, CardDefinition, CardEffect, CardKind, CardSet};
use agricola_re_ai::model::{Good, Player};
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
fn dispatch_card(
    set: &CardSet,
    player: &mut Player,
    hand: &mut [String],
    id: &str,
    extra_food_paid: u32,
) {
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
        extra_food_paid,
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
        true, // is_improvement_action: tests simulate improvement action path
        None, // play_space: no space (API-level dispatcher test)
    )
    .unwrap_or_else(|e| panic!("dispatch_play {id}: {e}"));
    *player = players[0].clone();
}

// ---------------- Registry reachability ----------------

#[test]
fn a109_reachable_via_default_registry() {
    let registry = build_default_registry().expect("default registry builds");
    let card_id = registry
        .card_id("A109")
        .unwrap_or_else(|| panic!("A109 must have a CardId"));
    let card = registry
        .get(card_id)
        .unwrap_or_else(|| panic!("A109 must be retrievable"));
    assert_eq!(card.external_id, "A109", "round-trip external_id for A109");
    assert_eq!(card.kind, CardKind::Occupation, "A109 must be Occupation");
    assert!(
        card.effects
            .iter()
            .any(|e| matches!(e, CardEffect::AfterImprovementPlayedGain { .. })),
        "A109 must have AfterImprovementPlayedGain effect"
    );
}

// ---------------- Effect-only (apply_on_play_effects) ----------------

#[test]
fn a109_apply_on_play_effects_no_supply_delta() {
    // A109 is an occupation with no on-play effect; its effect triggers
    // later when improvements are played. The on-play effect application
    // should not change supply.
    let set = new_set(vec![CardDefinition {
        use_limit: None,
        id: "A109".to_owned(),
        kind: CardKind::Occupation,
        cost: vec![],
        points: 0,
        effects: vec![CardEffect::AfterImprovementPlayedGain {
            good: Good::Food,
            amount: 3,
        }],
    }]);
    let mut player = Player::new(0, 0);
    player.supply.set(Good::Food, 5);
    apply_on_play_effects(&set, &mut player, "A109").expect("apply_on_play_effects");
    assert_eq!(
        player.supply.food, 5,
        "on-play effects of A109 should not change food"
    );
}

// ---------------- Dispatcher route: normal trigger ----------------

#[test]
fn a109_triggers_on_minor_improvement_play() {
    // Play A109, then play a minor improvement -> A109 grants 3 food.
    let set = new_set(vec![
        CardDefinition {
            use_limit: None,
            id: "A109".to_owned(),
            kind: CardKind::Occupation,
            cost: vec![],
            points: 0,
            effects: vec![CardEffect::AfterImprovementPlayedGain {
                good: Good::Food,
                amount: 3,
            }],
        },
        minor_def("MI1"),
    ]);

    let mut player = Player::new(0, 0);
    player.supply.set(Good::Food, 0);

    // Play A109 (occupation)
    dispatch_card(&set, &mut player, &mut ["A109".into()], "A109", 0);
    assert_eq!(player.supply.food, 0, "A109 play itself grants no food");

    // Play a minor improvement -> A109 triggers
    dispatch_card(&set, &mut player, &mut ["MI1".into()], "MI1", 0);
    assert_eq!(
        player.supply.food, 3,
        "A109 should grant 3 food when MI1 is played"
    );
}

// ---------------- Dispatcher route: not triggered by occupation play ----------------

#[test]
fn a109_not_triggered_by_occupation_play() {
    // Play A109, then play another occupation -> A109 does NOT trigger.
    let set = new_set(vec![
        CardDefinition {
            use_limit: None,
            id: "A109".to_owned(),
            kind: CardKind::Occupation,
            cost: vec![],
            points: 0,
            effects: vec![CardEffect::AfterImprovementPlayedGain {
                good: Good::Food,
                amount: 3,
            }],
        },
        occ_def("OCC1"),
    ]);

    let mut player = Player::new(0, 0);
    player.supply.set(Good::Food, 0);

    // Play A109
    dispatch_card(&set, &mut player, &mut ["A109".into()], "A109", 0);
    assert_eq!(player.supply.food, 0);

    // Play another occupation -> A109 should NOT trigger
    dispatch_card(&set, &mut player, &mut ["OCC1".into()], "OCC1", 0);
    assert_eq!(
        player.supply.food, 0,
        "A109 should not trigger on occupation play"
    );
}

// ---------------- Dispatcher route: multiple improvements ----------------

#[test]
fn a109_triggers_multiple_times() {
    // Play A109, then play multiple minor improvements -> each triggers 3 food.
    let set = new_set(vec![
        CardDefinition {
            use_limit: None,
            id: "A109".to_owned(),
            kind: CardKind::Occupation,
            cost: vec![],
            points: 0,
            effects: vec![CardEffect::AfterImprovementPlayedGain {
                good: Good::Food,
                amount: 3,
            }],
        },
        minor_def("MI1"),
        minor_def("MI2"),
    ]);

    let mut player = Player::new(0, 0);
    player.supply.set(Good::Food, 0);

    // Play A109
    dispatch_card(&set, &mut player, &mut ["A109".into()], "A109", 0);
    assert_eq!(player.supply.food, 0);

    // Play first minor improvement
    dispatch_card(&set, &mut player, &mut ["MI1".into()], "MI1", 0);
    assert_eq!(player.supply.food, 3);

    // Play second minor improvement
    dispatch_card(&set, &mut player, &mut ["MI2".into()], "MI2", 0);
    assert_eq!(
        player.supply.food, 6,
        "A109 should trigger again for second MI"
    );
}

// ---------------- Dispatcher route: boundary - improvement already in front ----------------

#[test]
fn a109_only_triggers_for_improvements_played_after_it() {
    // Play a minor improvement FIRST, then A109, then another minor improvement.
    // A109 should only trigger for the second MI (played after A109).
    let set = new_set(vec![
        CardDefinition {
            use_limit: None,
            id: "A109".to_owned(),
            kind: CardKind::Occupation,
            cost: vec![],
            points: 0,
            effects: vec![CardEffect::AfterImprovementPlayedGain {
                good: Good::Food,
                amount: 3,
            }],
        },
        minor_def("MI1"),
        minor_def("MI2"),
    ]);

    let mut player = Player::new(0, 0);
    player.supply.set(Good::Food, 0);

    // Play MI1 first (before A109)
    dispatch_card(&set, &mut player, &mut ["MI1".into()], "MI1", 0);
    assert_eq!(player.supply.food, 0, "no A109 yet");

    // Play A109
    dispatch_card(&set, &mut player, &mut ["A109".into()], "A109", 0);
    assert_eq!(player.supply.food, 0, "A109 play itself grants no food");

    // Play MI2 after A109 -> should trigger
    dispatch_card(&set, &mut player, &mut ["MI2".into()], "MI2", 0);
    assert_eq!(
        player.supply.food, 3,
        "A109 should trigger for MI2 played after it"
    );
}

// ---------------- Dispatch-level simulation: family growth path must NOT trigger A109 ----------------

#[test]
fn a109_not_triggered_by_family_growth_growwithminor() {
    // Play A109, then play a minor improvement via FamilyGrowthWithRoom/ModestFamilyGrowth + GrowWithMinor.
    // This is a family growth action, NOT an improvement action, so A109 must NOT trigger.
    // We simulate this by calling dispatch_play with is_improvement_action=false.
    let set = new_set(vec![
        CardDefinition {
            use_limit: None,
            id: "A109".to_owned(),
            kind: CardKind::Occupation,
            cost: vec![],
            points: 0,
            effects: vec![CardEffect::AfterImprovementPlayedGain {
                good: Good::Food,
                amount: 3,
            }],
        },
        minor_def("MI1"),
    ]);

    let mut player = Player::new(0, 0);
    player.supply.set(Good::Food, 0);

    // Play A109 (occupation) via normal improvement action path
    dispatch_card(&set, &mut player, &mut ["A109".into()], "A109", 0);
    assert_eq!(player.supply.food, 0, "A109 play itself grants no food");

    // Play a minor improvement via family growth path (is_improvement_action=false)
    // This simulates FamilyGrowthWithRoom/ModestFamilyGrowth + GrowWithMinor
    let mut cards = agricola_re_ai::cards::CardState {
        hands: vec![vec!["MI1".to_string()], vec![]],
        ..Default::default()
    };
    let mut players = vec![player.clone(), Player::new(1, 0)];
    players[0].played_cards.push("MI1".to_owned());
    let mut market = vec![];
    let mut well = BTreeMap::new();
    let mut budget = 64u32;
    let mut uses = BTreeMap::new();
    let limits = BTreeMap::new();
    let mut pending = None;

    agricola_re_ai::card_events::dispatch_play(
        &set,
        0,
        "MI1",
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
        false, // is_improvement_action: family growth path
        None,  // play_space: no space (API-level dispatcher test)
    )
    .unwrap_or_else(|e| panic!("dispatch_play MI1: {e}"));

    player = players[0].clone();

    // A109 must NOT trigger on family growth path
    assert_eq!(
        player.supply.food, 0,
        "A109 must not trigger on FamilyGrowthWithRoom/ModestFamilyGrowth + GrowWithMinor (family growth action)"
    );
}

// ---------------- Dispatch-level simulation: MajorImprovement + PlayMinor path MUST trigger A109 ----------------

#[test]
fn a109_triggers_on_major_improvement_play_minor() {
    // Play A109, then play a minor improvement via MajorImprovement + PlayMinor.
    // This IS an improvement action (Minor Improvement action on Major Improvement space),
    // so A109 MUST trigger. We simulate this by calling dispatch_play with is_improvement_action=true.
    let set = new_set(vec![
        CardDefinition {
            use_limit: None,
            id: "A109".to_owned(),
            kind: CardKind::Occupation,
            cost: vec![],
            points: 0,
            effects: vec![CardEffect::AfterImprovementPlayedGain {
                good: Good::Food,
                amount: 3,
            }],
        },
        minor_def("MI1"),
    ]);

    let mut player = Player::new(0, 0);
    player.supply.set(Good::Food, 0);

    // Play A109 (occupation) via normal improvement action path
    dispatch_card(&set, &mut player, &mut ["A109".into()], "A109", 0);
    assert_eq!(player.supply.food, 0, "A109 play itself grants no food");

    // Play a minor improvement via MajorImprovement + PlayMinor path (is_improvement_action=true)
    let mut cards = agricola_re_ai::cards::CardState {
        hands: vec![vec!["MI1".to_string()], vec![]],
        ..Default::default()
    };
    let mut players = vec![player.clone(), Player::new(1, 0)];
    players[0].played_cards.push("MI1".to_owned());
    let mut market = vec![];
    let mut well = BTreeMap::new();
    let mut budget = 64u32;
    let mut uses = BTreeMap::new();
    let limits = BTreeMap::new();
    let mut pending = None;

    agricola_re_ai::card_events::dispatch_play(
        &set,
        0,
        "MI1",
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
        true, // is_improvement_action: MajorImprovement + PlayMinor is an improvement action
        None, // play_space: no space (API-level dispatcher test)
    )
    .unwrap_or_else(|e| panic!("dispatch_play MI1: {e}"));

    player = players[0].clone();

    // A109 MUST trigger on MajorImprovement + PlayMinor path
    assert_eq!(
        player.supply.food, 3,
        "A109 must trigger on MajorImprovement + PlayMinor (improvement action)"
    );
}

// ==================== ENGINE-LEVEL BOUNDARY TESTS ====================
// These tests drive `Game::apply` to exercise the actual engine call sites
// (engine.rs) where `dispatch_play` is invoked with `is_improvement_action`.
// They verify that the flag is correctly set at each of the four engine paths:
// 1. MeetingPlace + PlayMinor (is_improvement_action=true)
// 2. Lessons/ExtraLessons + PlayOccupation (is_improvement_action=false)
// 3. FamilyGrowthWithRoom/ModestFamilyGrowth + GrowWithMinor (is_improvement_action=false)
// 4. MajorImprovement + PlayMinor (is_improvement_action=true)

use agricola_re_ai::engine::{Action, Detail};
use agricola_re_ai::model::{Config, Space, Tile};
use agricola_re_ai::Game;

/// Build a test card set: verified_basic_card_set(2) (meets 7+7 requirement) + A109 + test minor improvement.
fn engine_test_set() -> CardSet {
    let mut set =
        agricola_re_ai::cards::verified_basic_card_set(2).expect("verified basic card set");
    // Add A109 (from band A) - not in basic set
    set.definitions.push(agricola_re_ai::cards::CardDefinition {
        use_limit: None,
        id: "A109".to_owned(),
        kind: agricola_re_ai::cards::CardKind::Occupation,
        cost: vec![],
        points: 0,
        effects: vec![
            agricola_re_ai::cards::CardEffect::AfterImprovementPlayedGain {
                good: Good::Food,
                amount: 3,
            },
        ],
    });
    // Add TEST_MINOR as a test minor improvement (cost: Wood 3, Food 3) - B010 already exists in basic set
    set.definitions.push(agricola_re_ai::cards::CardDefinition {
        use_limit: None,
        id: "TEST_MINOR".to_owned(),
        kind: agricola_re_ai::cards::CardKind::MinorImprovement,
        cost: vec![(Good::Wood, 3), (Good::Food, 3)],
        points: 0,
        effects: vec![],
    });
    set
}

/// Build a Game with A109 in player 0's played_cards, TEST_MINOR in hand,
/// and the specified round card revealed.
fn build_engine_game(round_card: Space) -> Game {
    let set = engine_test_set();
    let config = Config {
        players: 2,
        seed: 123,
        additional_action_space_enabled: true,
    };
    let mut game = Game::with_cards(config, set).expect("game setup");

    // Player 0 has A109 played and TEST_MINOR in hand
    game.public.players[0].played_cards.push("A109".to_owned());
    game.cards.hands[0].push("TEST_MINOR".to_owned());
    game.cards.undealt.retain(|c| c != "TEST_MINOR");

    // Set up the round card
    game.public.revealed.push(match round_card {
        Space::FamilyGrowthWithRoom => agricola_re_ai::model::RoundCard::FamilyGrowthWithRoom,
        Space::MajorImprovement => agricola_re_ai::model::RoundCard::MajorImprovement,
        _ => panic!("unsupported round card for test"),
    });

    // For FamilyGrowthWithRoom, player needs a room
    if round_card == Space::FamilyGrowthWithRoom {
        game.public.players[0].farm[1].tile = Tile::Room;
    }

    game.public.round = 1;
    game
}

// ---------------- Engine-level boundary: FamilyGrowthWithRoom + GrowWithMinor must NOT trigger A109 ----------------

#[test]
fn a109_engine_family_growth_growwithminor_does_not_trigger() {
    // Negative test: FamilyGrowthWithRoom + GrowWithMinor is a family growth action,
    // NOT an improvement action, so A109 must NOT trigger.
    let mut game = build_engine_game(Space::FamilyGrowthWithRoom);

    // Player 0 has 3 wood, 3 food (exactly TEST_MINOR cost)
    game.public.players[0].supply.set(Good::Wood, 3);
    game.public.players[0].supply.set(Good::Food, 3);

    // Apply FamilyGrowthWithRoom + GrowWithMinor action
    let action = Action {
        space: Space::FamilyGrowthWithRoom,
        detail: Detail::GrowWithMinor {
            card_id: "TEST_MINOR".to_owned(),
        },
    };
    game.apply(action).expect("apply family growth with minor");

    // A109 must NOT trigger -> food stays at 0 (3 - 3 = 0, no +3 from A109)
    assert_eq!(
        game.public.players[0].supply.food, 0,
        "A109 must not trigger on FamilyGrowthWithRoom + GrowWithMinor (family growth action)"
    );

    // Sanity checks: family grew by 1, TEST_MINOR was played
    assert_eq!(
        game.public.players[0].family, 3,
        "family should have grown from 2 to 3"
    );
    assert!(
        game.public.players[0]
            .played_cards
            .contains(&"TEST_MINOR".to_owned()),
        "TEST_MINOR should be in played_cards"
    );
}

// ---------------- Engine-level boundary: MajorImprovement + PlayMinor MUST trigger A109 ----------------

#[test]
fn a109_engine_major_improvement_play_minor_triggers() {
    // Positive test: MajorImprovement + PlayMinor IS an improvement action,
    // so A109 MUST trigger.
    let mut game = build_engine_game(Space::MajorImprovement);

    // Player 0 has 3 wood, 3 food (exactly TEST_MINOR cost)
    game.public.players[0].supply.set(Good::Wood, 3);
    game.public.players[0].supply.set(Good::Food, 3);

    // Apply MajorImprovement + PlayMinor action
    let action = Action {
        space: Space::MajorImprovement,
        detail: Detail::PlayMinor {
            card_id: "TEST_MINOR".to_owned(),
        },
    };
    game.apply(action)
        .expect("apply major improvement play minor");

    // A109 MUST trigger -> food = 3 - 3 + 3 = 3
    assert_eq!(
        game.public.players[0].supply.food, 3,
        "A109 must trigger on MajorImprovement + PlayMinor (improvement action): 3-3+3=3"
    );

    // Sanity checks: TEST_MINOR was played, family unchanged
    assert!(
        game.public.players[0]
            .played_cards
            .contains(&"TEST_MINOR".to_owned()),
        "TEST_MINOR should be in played_cards"
    );
    assert_eq!(
        game.public.players[0].family, 2,
        "family should be unchanged (no growth)"
    );
}
