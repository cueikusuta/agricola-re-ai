//! Engine-level acceptance for the event-time bonus-point cards of the second
//! bonus chapter: E099 無関心な親 (Uncaring Parents, award at the end of each
//! harvest) and E100 博物館の館長 (Museum Caretaker, award at the start of each
//! work phase).
//!
//! Both are played through the real engine (Lessons space) and their awards are
//! reached by playing whole rounds, so this covers the production path
//! `apply -> start_round / harvest -> dispatch_start_round /
//! dispatch_harvest_end -> cards::store_event_bonus_points`, plus scoring,
//! save/load and tamper detection. The three-path (cards.rs / dispatcher /
//! legacy) comparison lives next to the dispatcher in `src/card_events.rs`.
//!
//! Feeding is the only decision a whole-round loop cannot make on its own, so
//! the tests stock both seats with food; everything else uses the legal detail
//! policies. Two tests set the house or the stock directly to reach a condition
//! a short game cannot build (a stone house), which makes the event log
//! inconsistent with the state — those tests therefore do not save/load. The
//! save/load and tamper checks run on the naturally played E100 game.

use agricola_re_ai::cards::{verified_band_e_cards, verified_basic_card_set, CardKind};
use agricola_re_ai::engine::{Action, Detail};
use agricola_re_ai::model::{Config, Game, Good, House, Space};
use agricola_re_ai::policy::{CardChoiceDetails, DetailPolicy, SimpleDetails};

/// The six goods E100 counts (food and animals do not count).
const LISTED_GOODS: [Good; 6] = [
    Good::Wood,
    Good::Clay,
    Good::Reed,
    Good::Stone,
    Good::Grain,
    Good::Vegetable,
];

/// Two-player game where seat 0 holds E099 and E100. The rest of the deal is
/// filled from the A/B preset plus the E band so `CardSet`'s "one location per
/// card" invariant holds. Both seats start with food so the harvest's feeding
/// phase never needs a decision this test does not model.
fn game() -> Game {
    let mut set = verified_basic_card_set(2).unwrap();
    set.definitions.extend(verified_band_e_cards());
    let ids = |kind: CardKind| -> Vec<String> {
        set.definitions
            .iter()
            .filter(|card| card.kind == kind)
            .map(|card| card.id.clone())
            .collect()
    };
    let occupations = ids(CardKind::Occupation);
    let minors = ids(CardKind::MinorImprovement);
    assert!(occupations.len() >= 16 && minors.len() >= 14);

    let mut seat0 = vec!["E099".to_string(), "E100".to_string()];
    let rest: Vec<String> = occupations
        .iter()
        .filter(|id| !seat0.contains(id))
        .take(5)
        .cloned()
        .collect();
    seat0.extend(rest);
    let seat1: Vec<String> = occupations
        .iter()
        .filter(|id| !seat0.contains(id))
        .take(7)
        .cloned()
        .collect();
    let seat0_minors: Vec<String> = minors.iter().take(7).cloned().collect();
    let seat1_minors: Vec<String> = minors
        .iter()
        .filter(|id| !seat0_minors.contains(id))
        .take(7)
        .cloned()
        .collect();
    assert_eq!(seat0.len(), 7);
    assert_eq!(seat1.len(), 7);
    assert_eq!(seat0_minors.len(), 7);
    assert_eq!(seat1_minors.len(), 7);
    let hands = vec![
        [seat0, seat0_minors].concat(),
        [seat1, seat1_minors].concat(),
    ];

    let game = Game::with_cards_and_hands_with_start_player(
        Config {
            players: 2,
            seed: 99,
            additional_action_space_enabled: false,
        },
        set,
        hands,
        0,
    )
    .unwrap();
    game.validate().unwrap();
    game
}

/// The same game with both seats stocked with food, so the harvest feeding
/// phase needs no decision. Injecting food after construction makes the event
/// log inconsistent with the state, so these games must not be saved.
fn game_with_food() -> Game {
    let mut game = game();
    for player in &mut game.public.players {
        player.supply.set(Good::Food, 24);
    }
    game.validate().unwrap();
    game
}

fn play_occupation(game: &mut Game, card_id: &str) {
    game.apply(Action {
        space: Space::Lessons,
        detail: Detail::PlayOccupation {
            card_id: card_id.to_owned(),
        },
    })
    .unwrap();
}

/// Play one legal action with the same detail policies the tests use elsewhere.
fn play_one_action(game: &mut Game) {
    let seat = game.public.active_player;
    let obs = game.observation(seat).expect("active seat observation");
    let legal = game.legal_spaces();
    let space = legal
        .into_iter()
        .find(|space| {
            CardChoiceDetails.produce(&obs, *space).is_some()
                || SimpleDetails.produce(&obs, *space).is_some()
        })
        .expect("a legal space");
    let action = CardChoiceDetails
        .produce(&obs, space)
        .or_else(|| SimpleDetails.produce(&obs, space))
        .expect("legal detail");
    game.apply(action).expect("legal action applies");
}

/// Play until `round` starts. Rounds 4 and later cross a harvest, so this also
/// exercises the end-of-harvest chain.
fn advance_to_round(game: &mut Game, round: u8) {
    let mut guard = 0;
    while game.public.round < round {
        assert!(!game.public.finished, "the game must not finish early");
        play_one_action(game);
        guard += 1;
        assert!(
            guard < 120,
            "failed to reach round {round} (stuck in round {})",
            game.public.round
        );
    }
}

/// Play every worker of the current round except the last one, so the caller
/// can set up the state the next round start will observe.
fn play_until_last_worker(game: &mut Game) {
    let mut guard = 0;
    loop {
        let quota: u16 = game.public.work_quota.iter().map(|q| u16::from(*q)).sum();
        let placed: u16 = game
            .public
            .workers_placed
            .iter()
            .map(|q| u16::from(*q))
            .sum();
        if placed + 1 >= quota {
            return;
        }
        play_one_action(game);
        guard += 1;
        assert!(guard < 40, "failed to reach the round's last worker");
    }
}

/// Take the round's last action from a space that yields food only, so a state
/// prepared for the next round start is not polluted by the action itself.
fn finish_round_with_a_food_only_action(game: &mut Game) {
    let legal = game.legal_spaces();
    let space = [Space::DayLaborer, Space::Fishing, Space::TravelingPlayers]
        .into_iter()
        .find(|space| legal.contains(space))
        .expect("a food-only space must be legal for the round's last action");
    game.apply(Action {
        space,
        detail: Detail::None,
    })
    .expect("food-only last action");
}

fn has_listed_good(game: &Game, seat: usize) -> bool {
    LISTED_GOODS
        .iter()
        .any(|good| game.public.players[seat].supply.get(*good) > 0)
}

#[test]
fn e099_pays_one_point_at_each_harvest_end_in_a_stone_house() {
    let mut game = game_with_food();
    play_occupation(&mut game, "E099");
    assert_eq!(
        game.stored_bonus_points(0),
        0,
        "nothing is paid before the first harvest"
    );
    // The stone house is the only condition. It is set directly instead of
    // modelling two renovations that a four-round test cannot afford; the
    // engine resolves the award the same way either way.
    game.public.players[0].house = House::Stone;
    game.validate().unwrap();

    advance_to_round(&mut game, 4);
    play_until_last_worker(&mut game);
    finish_round_with_a_food_only_action(&mut game);
    assert_eq!(game.public.round, 5);
    assert_eq!(
        game.stored_bonus_points(0),
        1,
        "the round-4 harvest end pays exactly 1 point"
    );
    assert_eq!(
        game.public.players[0].card_points, 0,
        "printed points stay 0"
    );
    game.validate().unwrap();

    advance_to_round(&mut game, 7);
    play_until_last_worker(&mut game);
    finish_round_with_a_food_only_action(&mut game);
    assert_eq!(game.public.round, 8);
    assert_eq!(
        game.stored_bonus_points(0),
        2,
        "the round-7 harvest end pays again"
    );
    assert_eq!(
        game.stored_bonus_points(1),
        0,
        "the other seat never gains from seat 0's card"
    );

    let standings = game.standings();
    assert_eq!(
        standings.scores[0].categories["card_bonus"], 2,
        "stored points join the card_bonus category"
    );
    assert_eq!(standings.scores[0].categories["cards"], 0);
}

#[test]
fn e099_pays_nothing_outside_a_stone_house() {
    let mut game = game_with_food();
    play_occupation(&mut game, "E099");
    assert_eq!(game.public.players[0].house, House::Wood);
    advance_to_round(&mut game, 4);
    play_until_last_worker(&mut game);
    finish_round_with_a_food_only_action(&mut game);
    assert_eq!(
        game.stored_bonus_points(0),
        0,
        "a wooden house earns no harvest-end points"
    );
    assert!(
        !game.bonus_points.contains_key(&0),
        "no ledger entry is created for a failed condition"
    );
}

#[test]
fn e100_pays_at_the_work_phase_start_when_a_listed_good_is_in_stock() {
    let mut game = game();
    play_occupation(&mut game, "E100");
    advance_to_round(&mut game, 2);
    // The round-2 start is the beginning of the work phase: no round-2 action
    // has happened yet, so this is exactly the state the award observed.
    assert!(
        has_listed_good(&game, 0),
        "the round-2 start must hold a listed good for this test"
    );
    assert_eq!(
        game.stored_bonus_points(0),
        1,
        "one listed good at the work-phase start earns 1 point"
    );
    assert_eq!(
        game.bonus_points
            .get(&0)
            .and_then(|cards| cards.get("E100"))
            .copied(),
        Some(1),
        "the award cites the card that earned it"
    );
    game.validate().unwrap();

    let standings = game.standings();
    assert_eq!(standings.scores[0].categories["card_bonus"], 1);
    assert_eq!(standings.scores[0].categories["cards"], 0);

    // The ledger is bookkeeping outside `state_hash`; save/load round-trips it
    // through the stamped integrity hash.
    let saved = game.save_json().unwrap();
    let loaded = Game::load_json(&saved).unwrap();
    assert_eq!(loaded.bonus_points, game.bonus_points);
    assert_eq!(loaded.standings(), game.standings());
    assert_eq!(loaded.state_hash(), game.state_hash());

    // A hand-edited amount must be rejected.
    let mut tampered: serde_json::Value = serde_json::from_str(&saved).unwrap();
    tampered["bonus_points"]["0"]["E100"] = serde_json::json!(4);
    assert_eq!(
        Game::load_json(&tampered.to_string()).unwrap_err(),
        "saved game bookkeeping integrity mismatch"
    );
}

#[test]
fn e100_pays_nothing_when_only_food_is_in_stock() {
    let mut game = game_with_food();
    play_occupation(&mut game, "E100");
    assert_eq!(
        game.stored_bonus_points(0),
        0,
        "the round-1 work phase had no listed good yet"
    );
    play_until_last_worker(&mut game);
    for good in LISTED_GOODS {
        game.public.players[0].supply.set(good, 0);
    }
    finish_round_with_a_food_only_action(&mut game);
    assert_eq!(game.public.round, 2);
    assert!(!has_listed_good(&game, 0), "only food is left in stock");
    assert_eq!(
        game.stored_bonus_points(0),
        0,
        "food does not count for E100"
    );
    assert!(!game.bonus_points.contains_key(&0));
}

#[test]
fn e099_and_e100_are_not_part_of_the_two_player_preset() {
    let preset = verified_basic_card_set(2).unwrap();
    assert_eq!(preset.definitions.len(), 35);
    for id in ["E099", "E100"] {
        assert!(
            verified_band_e_cards().iter().any(|card| card.id == id),
            "{id} is in the E band pool"
        );
        assert!(
            !preset.definitions.iter().any(|card| card.id == id),
            "{id} must stay out of the A/B preset"
        );
    }
}
