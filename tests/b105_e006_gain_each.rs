//! Tests for B105 箱造り (Case Builder) and E006 再集計 (Recount) through the
//! real engine paths.
//!
//! Both cards share the `OnPlayGainEachWithAtLeast { goods, threshold, amount }`
//! variant: for each good in `goods`, if the owner already holds at least
//! `threshold` of it, gain `amount` of that good; goods below threshold grant
//! nothing. Judged per good against the owner's own supply (not resource
//! totals).
//!
//! - B105: food/grain/vegetable/reed/wood, threshold 2, amount 1. No pass.
//! - E006: wood/clay/reed/stone, threshold 4, amount 1, then passes the card
//!   to the left neighbor's hand (PostPlayPassLeft); the card does not remain
//!   in the owner's played_cards.

use agricola_re_ai::cards::{verified_band_b_cards, verified_band_e_cards, CardKind, CardSet};
use agricola_re_ai::engine::{Action, Detail};
use agricola_re_ai::model::{Config, Game, Good, Space};
use agricola_re_ai::registry::build_default_registry;

fn set_with_b105() -> CardSet {
    let mut set = agricola_re_ai::cards::verified_basic_card_set(2).unwrap();
    set.definitions.push(
        verified_band_b_cards()
            .into_iter()
            .find(|c| c.id == "B105")
            .expect("B105 must be defined in verified_band_b_cards"),
    );
    set
}

fn set_with_e006() -> CardSet {
    let mut set = agricola_re_ai::cards::verified_basic_card_set(2).unwrap();
    set.definitions.push(
        verified_band_e_cards()
            .into_iter()
            .find(|c| c.id == "E006")
            .expect("E006 must be defined in verified_band_e_cards"),
    );
    set
}

/// Move `card_id` to seat 0's hand, removing it from every other zone.
fn put_in_hand(game: &mut Game, card_id: &str) {
    for hand in game.cards.hands.iter_mut() {
        hand.retain(|id| id != card_id);
    }
    game.cards.undealt.retain(|id| id != card_id);
    game.cards.hands[0].push(card_id.to_owned());
}

/// Deal every card of `set` between the two seats, with `card_id` in seat 0.
/// Each seat receives exactly 7 occupations and 7 minor improvements; any
/// remaining cards stay undealt.
fn hands_with_card(set: &CardSet, card_id: &str) -> Vec<Vec<String>> {
    let occupations: Vec<String> = set
        .definitions
        .iter()
        .filter(|c| c.kind == CardKind::Occupation)
        .map(|c| c.id.clone())
        .collect();
    let minors: Vec<String> = set
        .definitions
        .iter()
        .filter(|c| c.kind == CardKind::MinorImprovement)
        .map(|c| c.id.clone())
        .collect();
    let is_occ = occupations.iter().any(|id| id == card_id);
    let mut seat0_occ: Vec<String> = Vec::new();
    if is_occ {
        seat0_occ.push(card_id.to_owned());
    }
    seat0_occ.extend(
        occupations
            .iter()
            .filter(|id| *id != card_id)
            .take(7 - seat0_occ.len())
            .cloned(),
    );
    let mut seat0_minor: Vec<String> = Vec::new();
    if !is_occ {
        seat0_minor.push(card_id.to_owned());
    }
    seat0_minor.extend(
        minors
            .iter()
            .filter(|id| *id != card_id)
            .take(7 - seat0_minor.len())
            .cloned(),
    );
    assert_eq!(seat0_occ.len(), 7, "seat 0 must have 7 occupations");
    assert_eq!(
        seat0_minor.len(),
        7,
        "seat 0 must have 7 minor improvements"
    );
    let used_occ: std::collections::HashSet<&str> = seat0_occ.iter().map(|s| s.as_str()).collect();
    let used_minor: std::collections::HashSet<&str> =
        seat0_minor.iter().map(|s| s.as_str()).collect();
    let seat1_occ: Vec<String> = occupations
        .iter()
        .filter(|id| !used_occ.contains(id.as_str()))
        .take(7)
        .cloned()
        .collect();
    let seat1_minor: Vec<String> = minors
        .iter()
        .filter(|id| !used_minor.contains(id.as_str()))
        .take(7)
        .cloned()
        .collect();
    vec![
        [seat0_occ, seat0_minor].concat(),
        [seat1_occ, seat1_minor].concat(),
    ]
}

#[test]
fn b105_reachable_via_default_registry() {
    let registry = build_default_registry().expect("default registry builds");
    let card_id = registry.card_id("B105").expect("B105 must be registered");
    let definition = registry
        .get(card_id)
        .expect("registered B105 resolves")
        .to_card_definition();
    assert_eq!(definition.kind, agricola_re_ai::cards::CardKind::Occupation);
    assert_eq!(definition.cost, vec![]);
    assert_eq!(
        definition.effects,
        vec![
            agricola_re_ai::cards::CardEffect::OnPlayGainEachWithAtLeast {
                goods: vec![
                    Good::Food,
                    Good::Grain,
                    Good::Vegetable,
                    Good::Reed,
                    Good::Wood,
                ],
                threshold: 2,
                amount: 1,
            }
        ]
    );
}

#[test]
fn e006_reachable_via_default_registry() {
    let registry = build_default_registry().expect("default registry builds");
    let card_id = registry.card_id("E006").expect("E006 must be registered");
    let definition = registry
        .get(card_id)
        .expect("registered E006 resolves")
        .to_card_definition();
    assert_eq!(
        definition.kind,
        agricola_re_ai::cards::CardKind::MinorImprovement
    );
    assert_eq!(definition.cost, vec![]);
    assert_eq!(
        definition.effects,
        vec![
            agricola_re_ai::cards::CardEffect::OnPlayGainEachWithAtLeast {
                goods: vec![Good::Wood, Good::Clay, Good::Reed, Good::Stone],
                threshold: 4,
                amount: 1,
            },
            agricola_re_ai::cards::CardEffect::PostPlayPassLeft,
        ]
    );
}

/// B105-normal: goods at or above threshold 2 grant 1 each; goods at 1 grant
/// nothing. Cost 0, no fee (first occupation via Lessons).
#[test]
fn b105_normal_grants_each_good_at_least_threshold() {
    let config = Config {
        players: 2,
        seed: 7,
        additional_action_space_enabled: false,
    };
    let set = set_with_b105();
    let mut game = Game::with_cards(config, set.clone()).unwrap();
    game.start_round().unwrap();
    put_in_hand(&mut game, "B105");
    game.public.active_player = 0;
    // food/grain/reed at threshold 2 -> +1 each; vegetable/wood at 1 -> +0.
    game.public.players[0].supply.food = 2;
    game.public.players[0].supply.grain = 2;
    game.public.players[0].supply.vegetable = 1;
    game.public.players[0].supply.reed = 2;
    game.public.players[0].supply.wood = 1;

    game.apply(Action {
        space: Space::Lessons,
        detail: Detail::PlayOccupation {
            card_id: "B105".to_owned(),
        },
    })
    .expect("B105 play via Lessons");

    let p = &game.public.players[0];
    assert_eq!(p.supply.food, 3, "food at threshold 2 grants +1");
    assert_eq!(p.supply.grain, 3, "grain at threshold 2 grants +1");
    assert_eq!(p.supply.vegetable, 1, "vegetable below threshold grants +0");
    assert_eq!(p.supply.reed, 3, "reed at threshold 2 grants +1");
    assert_eq!(p.supply.wood, 1, "wood below threshold grants +0");
    assert!(p.played_cards.iter().any(|id| id == "B105"));
    assert!(!game.cards.hands[0].iter().any(|id| id == "B105"));
}

/// B105 boundary: exactly 2 grants +1; below 2 grants +0.
#[test]
fn b105_boundary_exactly_threshold_grants() {
    let config = Config {
        players: 2,
        seed: 7,
        additional_action_space_enabled: false,
    };
    let set = set_with_b105();
    let mut game = Game::with_cards(config, set.clone()).unwrap();
    game.start_round().unwrap();
    put_in_hand(&mut game, "B105");
    game.public.active_player = 0;
    game.public.players[0].supply.food = 2; // exactly threshold
    game.public.players[0].supply.grain = 1; // below threshold
    game.public.players[0].supply.wood = 0;

    game.apply(Action {
        space: Space::Lessons,
        detail: Detail::PlayOccupation {
            card_id: "B105".to_owned(),
        },
    })
    .expect("B105 play via Lessons");

    let p = &game.public.players[0];
    assert_eq!(p.supply.food, 3, "exactly threshold grants +1");
    assert_eq!(p.supply.grain, 1, "below threshold grants +0");
    assert_eq!(p.supply.wood, 0, "zero grants +0");
}

/// E006-normal: all four building resources at threshold 4 grant +1 each,
/// then the card passes left and leaves the owner's played_cards.
#[test]
fn e006_normal_grants_each_and_passes_left() {
    let config = Config {
        players: 2,
        seed: 7,
        additional_action_space_enabled: false,
    };
    let set = set_with_e006();
    let mut game = Game::with_cards(config, set.clone()).unwrap();
    game.start_round().unwrap();
    put_in_hand(&mut game, "E006");
    game.public.active_player = 0;
    game.public.players[0].supply.wood = 4;
    game.public.players[0].supply.clay = 4;
    game.public.players[0].supply.reed = 4;
    game.public.players[0].supply.stone = 4;

    game.apply(Action {
        space: Space::MajorImprovement,
        detail: Detail::PlayMinor {
            card_id: "E006".to_owned(),
        },
    })
    .expect("E006 play via Major Improvement / PlayMinor");

    let p = &game.public.players[0];
    assert_eq!(p.supply.wood, 5, "wood at threshold 4 grants +1");
    assert_eq!(p.supply.clay, 5, "clay at threshold 4 grants +1");
    assert_eq!(p.supply.reed, 5, "reed at threshold 4 grants +1");
    assert_eq!(p.supply.stone, 5, "stone at threshold 4 grants +1");
    // Card passed left: not in owner's played_cards or hand.
    assert!(!p.played_cards.iter().any(|id| id == "E006"));
    assert!(!game.cards.hands[0].iter().any(|id| id == "E006"));
    // Left neighbor (seat 1) now holds it in hand.
    assert!(game.cards.hands[1].iter().any(|id| id == "E006"));
}

/// E006-zero: all resources below threshold 4 grant nothing; still passes left.
#[test]
fn e006_zero_below_threshold_grants_nothing_but_passes_left() {
    let config = Config {
        players: 2,
        seed: 7,
        additional_action_space_enabled: false,
    };
    let set = set_with_e006();
    let mut game = Game::with_cards(config, set.clone()).unwrap();
    game.start_round().unwrap();
    put_in_hand(&mut game, "E006");
    game.public.active_player = 0;
    game.public.players[0].supply.wood = 3;
    game.public.players[0].supply.clay = 3;
    game.public.players[0].supply.reed = 3;
    game.public.players[0].supply.stone = 3;

    game.apply(Action {
        space: Space::MajorImprovement,
        detail: Detail::PlayMinor {
            card_id: "E006".to_owned(),
        },
    })
    .expect("E006 play via Major Improvement / PlayMinor");

    let p = &game.public.players[0];
    assert_eq!(p.supply.wood, 3, "below threshold grants +0");
    assert_eq!(p.supply.clay, 3, "below threshold grants +0");
    assert_eq!(p.supply.reed, 3, "below threshold grants +0");
    assert_eq!(p.supply.stone, 3, "below threshold grants +0");
    assert!(!p.played_cards.iter().any(|id| id == "E006"));
    assert!(game.cards.hands[1].iter().any(|id| id == "E006"));
}

fn save_load_replay_card(
    card_id: &str,
    set: CardSet,
    action: Action,
    expect_in_owner_played: bool,
) {
    let config = Config {
        players: 2,
        seed: 7,
        additional_action_space_enabled: false,
    };
    let hands = hands_with_card(&set, card_id);
    let mut game =
        Game::with_cards_and_hands_with_start_player(config.clone(), set.clone(), hands.clone(), 0)
            .expect("game setup with card in seat 0's hand");
    game.public.active_player = 0;

    game.apply(action.clone()).expect("card play must complete");
    let before_hash = game.state_hash();

    // Save/load reproduces the state exactly (effect applied once).
    let loaded = Game::load_json(&game.save_json().expect("save recorded game"))
        .expect("saved game must resume");
    assert_eq!(
        loaded.state_hash(),
        before_hash,
        "resume must reproduce state hash"
    );
    assert_eq!(
        loaded.public.players[0]
            .played_cards
            .iter()
            .any(|id| id == card_id),
        expect_in_owner_played,
        "card zone must survive save/load"
    );

    // Full replay from setup + hands + start player + action log reproduces.
    let replayed = Game::replay_with_setup_and_start_player(
        config,
        set,
        Some(hands),
        Some(0),
        &game.log.events,
    )
    .expect("recorded game must replay");
    assert_eq!(
        replayed.state_hash(),
        before_hash,
        "replay must reproduce state hash"
    );
    assert_eq!(
        replayed.public.players[0]
            .played_cards
            .iter()
            .any(|id| id == card_id),
        expect_in_owner_played,
        "card zone must survive replay"
    );
}

#[test]
fn b105_save_load_and_replay() {
    let set = set_with_b105();
    save_load_replay_card(
        "B105",
        set,
        Action {
            space: Space::Lessons,
            detail: Detail::PlayOccupation {
                card_id: "B105".to_owned(),
            },
        },
        true, // B105 stays in the owner's played area (no pass-left)
    );
}

#[test]
fn e006_save_load_and_replay() {
    let set = set_with_e006();
    save_load_replay_card(
        "E006",
        set,
        Action {
            space: Space::MajorImprovement,
            detail: Detail::PlayMinor {
                card_id: "E006".to_owned(),
            },
        },
        false, // E006 passes left, so it leaves the owner's played_cards
    );
}
