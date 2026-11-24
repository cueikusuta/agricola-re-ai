//! Tests for B059 食料箱 (Food Chest) through the real engine paths.
//!
//! The band pools are deliberately kept out of the A/B two-player preset, so
//! each test appends the B059 definition to a verified-preset sandbox and
//! keeps the card partition valid while relocating the card (the same
//! teleport pattern the PR #385 review fixtures use).
//!
//! Covered per the spec test contract (data/bga-card-tests/B059.json):
//! - registry reachability (B059 resolves via `build_default_registry`)
//! - B059-normal: the Major Improvement action grants food 4 and pays wood 1
//! - B059-other: the Meeting Place minor-improvement action grants food 2
//! - the family-growth route (GrowWithMinor) also grants food 2
//! - other spaces (a plain worker placement) grant nothing
//! - resume/replay: a game that plays B059 through the Major Improvement
//!   action entirely through recorded legal actions; save/load and full
//!   setup/hands/action-log replay reproduce the exact state hash and food
//!   count without granting the effect twice
//!
//! The small focused fixtures use the established teleport pattern. The
//! resume/replay fixture does not: every post-setup transition is a
//! recorded `Game::apply` call, so its action log reconstructs the game
//! from the same setup and initial hands.

use agricola_re_ai::cards::{verified_band_b_cards, CardKind, CardSet};
use agricola_re_ai::engine::{Action, Detail};
use agricola_re_ai::model::{Config, Game, RoundCard, Space};
use agricola_re_ai::registry::build_default_registry;

fn b059_definition() -> agricola_re_ai::cards::CardDefinition {
    verified_band_b_cards()
        .into_iter()
        .find(|c| c.id == "B059")
        .expect("B059 must be defined in verified_band_b_cards")
}

/// Verified two-player preset plus the B059 definition appended.
fn set_with_b059() -> CardSet {
    let mut set = agricola_re_ai::cards::verified_basic_card_set(2).unwrap();
    set.definitions.push(b059_definition());
    set
}

/// Move B059 to exactly one location (a hand, or a played area for
/// `into_play`), keeping the "every implemented card occupies exactly one
/// zone" partition valid.
fn relocate_b059(game: &mut Game, into_play: bool) {
    for hand in game.cards.hands.iter_mut() {
        hand.retain(|id| id != "B059");
    }
    game.cards.undealt.retain(|id| id != "B059");
    if into_play {
        game.public.players[0].played_cards.push("B059".to_owned());
    } else {
        game.cards.hands[0].push("B059".to_owned());
    }
}

/// A two-player fixed-seed game with one started round.
fn fresh_game() -> (Game, CardSet) {
    let config = Config {
        players: 2,
        seed: 7,
        additional_action_space_enabled: false,
    };
    let set = set_with_b059();
    let mut game = Game::with_cards(config, set.clone()).unwrap();
    game.start_round().unwrap();
    relocate_b059(&mut game, false);
    game.public.active_player = 0;
    (game, set)
}

#[test]
fn b059_reachable_via_default_registry() {
    let registry = build_default_registry().expect("default registry builds");
    let card_id = registry
        .card_id("B059")
        .expect("B059 must be registered (band B pool)");
    let definition = registry
        .get(card_id)
        .expect("registered B059 resolves")
        .to_card_definition();
    assert_eq!(
        definition.kind,
        agricola_re_ai::cards::CardKind::MinorImprovement
    );
    assert_eq!(
        definition.cost,
        vec![(agricola_re_ai::model::Good::Wood, 1)]
    );
    assert_eq!(definition.points, 0);
}

/// B059-normal: the Major Improvement action (a minor played on the Major
/// Improvement space) grants 4 food and pays the wood 1 cost.
#[test]
fn b059_major_improvement_action_grants_four_food() {
    let (mut game, _set) = fresh_game();
    game.public.players[0].supply.wood = 1;
    game.public.players[0].supply.food = 0;

    game.apply(Action {
        space: Space::MajorImprovement,
        detail: Detail::PlayMinor {
            card_id: "B059".to_owned(),
        },
    })
    .expect("B059 play via the Major Improvement action");

    let p = &game.public.players[0];
    assert_eq!(
        p.supply.food, 4,
        "the Major Improvement action grants 4 food"
    );
    assert_eq!(p.supply.wood, 0, "cost wood 1 is paid");
    assert!(p.played_cards.iter().any(|id| id == "B059"));
    assert!(!game.cards.hands[0].iter().any(|id| id == "B059"));
}

/// B059-other: the Meeting Place minor-improvement action grants 2 food.
/// B059 is a minor improvement, yet the space — not the card kind —
/// decides the amount.
#[test]
fn b059_meeting_place_action_grants_two_food() {
    let (mut game, _set) = fresh_game();
    game.public.players[0].supply.wood = 1;
    game.public.players[0].supply.food = 0;

    game.apply(Action {
        space: Space::MeetingPlace,
        detail: Detail::PlayMinor {
            card_id: "B059".to_owned(),
        },
    })
    .expect("B059 play via the Meeting Place minor-improvement action");

    let p = &game.public.players[0];
    assert_eq!(
        p.supply.food, 2,
        "the Meeting Place action grants the other_amount (2 food)"
    );
    assert_eq!(p.supply.wood, 0, "cost wood 1 is paid");
    assert!(p.played_cards.iter().any(|id| id == "B059"));
}

/// The third improvement-play route (family growth with a minor from hand)
/// also resolves the non-major branch: 2 food.
#[test]
fn b059_family_growth_with_minor_grants_two_food() {
    let (mut game, _set) = fresh_game();
    // The family-growth route needs the FamilyGrowthWithRoom round card on
    // the board: reveal it for this fixture (the established teleport
    // pattern) so the space is legal.
    game.public.revealed.push(RoundCard::FamilyGrowthWithRoom);
    // Family growth needs a free room; grant one via the housing bonus the
    // same way B010 would (fixture teleport, focused effect check).
    game.public.players[0].extra_housing = 1;
    game.public.players[0].supply.wood = 1;
    game.public.players[0].supply.food = 0;
    let family_before = game.public.players[0].family;

    game.apply(Action {
        space: Space::FamilyGrowthWithRoom,
        detail: Detail::GrowWithMinor {
            card_id: "B059".to_owned(),
        },
    })
    .expect("B059 play via family growth");

    let p = &game.public.players[0];
    assert_eq!(p.supply.food, 2, "the family-growth route grants 2 food");
    assert_eq!(p.supply.wood, 0, "cost wood 1 is paid");
    assert_eq!(p.family, family_before + 1, "the family grew by one");
    assert!(p.played_cards.iter().any(|id| id == "B059"));
}

/// Other spaces must not fire the on-play gain: a plain worker placement
/// (Forest) grants no B059 food even with the card already played.
#[test]
fn b059_not_triggered_by_other_spaces() {
    let (mut game, _set) = fresh_game();
    relocate_b059(&mut game, true);
    game.public.players[0].supply.food = 0;

    game.apply(Action {
        space: Space::Forest,
        detail: Detail::None,
    })
    .expect("a plain forest placement must be legal");

    let p = &game.public.players[0];
    assert_eq!(
        p.supply.wood, 6,
        "the forest grants its usual 6 wood (engine Forest amount, unrelated to B059)"
    );
    assert_eq!(p.supply.food, 0, "B059 grants nothing on other spaces");
}

/// Explicit initial hands put B059 in seat 0 hand without changing any
/// post-setup state outside the action log.
fn hands_with_b059(set: &CardSet) -> Vec<Vec<String>> {
    let occupations: Vec<String> = set
        .definitions
        .iter()
        .filter(|card| card.kind == CardKind::Occupation)
        .map(|card| card.id.clone())
        .collect();
    let minors: Vec<String> = set
        .definitions
        .iter()
        .filter(|card| card.kind == CardKind::MinorImprovement && card.id != "B059")
        .map(|card| card.id.clone())
        .collect();
    assert!(occupations.len() >= 14 && minors.len() >= 13);

    let mut seat_zero = occupations[..7].to_vec();
    seat_zero.push("B059".to_owned());
    seat_zero.extend_from_slice(&minors[..6]);
    let seat_one = [occupations[7..14].to_vec(), minors[6..13].to_vec()].concat();
    vec![seat_zero, seat_one]
}

fn game_with_b059() -> (Config, CardSet, Vec<Vec<String>>, Game) {
    let config = Config {
        players: 2,
        seed: 2,
        additional_action_space_enabled: false,
    };
    let set = set_with_b059();
    let hands = hands_with_b059(&set);
    let game =
        Game::with_cards_and_hands_with_start_player(config.clone(), set.clone(), hands.clone(), 0)
            .expect("game setup with B059 in seat 0 hand");
    (config, set, hands, game)
}

const PLAIN_SPACES: [Space; 7] = [
    Space::Forest,
    Space::ClayPit,
    Space::ReedBank,
    Space::Fishing,
    Space::DayLaborer,
    Space::GrainSeeds,
    Space::VegetableSeeds,
];

fn first_plain_space(legal: &[Space]) -> Space {
    PLAIN_SPACES
        .iter()
        .copied()
        .find(|space| legal.contains(space))
        .expect("at least one detail-free space must be legal")
}

fn place_day_laborer() -> Action {
    Action {
        space: Space::DayLaborer,
        detail: Detail::None,
    }
}

/// Seed-2 real-action replay fixture driver.
///
/// Seat 0 takes Forest in round 1 to secure the wood 1 cost, then plays
/// B059 through the Major Improvement action space at round 2 (the seed-2
/// round card), which must grant 4 food once. Both seats then keep food at
/// or above the 2-member feeding cost (4 food) with Day Laborer so no
/// begging card disturbs the final state.
///
/// Every post-setup transition is a recorded `Game::apply` call, so the
/// action log reconstructs the game exactly. save/load and full replay must
/// reproduce the identical state hash and food count, proving B059's
/// space-dependent grant fires exactly once and never doubles on
/// resume/replay.
fn seat_zero_action(game: &Game, legal: &[Space]) -> Action {
    let player = &game.public.players[0];
    let b059_played = player.played_cards.iter().any(|id| id == "B059");

    // Before B059 is played, secure the 1 wood needed to pay its cost.
    if !b059_played && player.supply.wood < 1 && legal.contains(&Space::Forest) {
        return Action {
            space: Space::Forest,
            detail: Detail::None,
        };
    }

    // Play B059 through the Major Improvement action space (needs 1 wood).
    if !b059_played && player.supply.wood >= 1 && legal.contains(&Space::MajorImprovement) {
        return Action {
            space: Space::MajorImprovement,
            detail: Detail::PlayMinor {
                card_id: "B059".to_owned(),
            },
        };
    }

    // Keep food at or above the 2-member feeding cost (4 food).
    if player.supply.food < 4 && legal.contains(&Space::DayLaborer) {
        return place_day_laborer();
    }

    Action {
        space: first_plain_space(legal),
        detail: Detail::None,
    }
}

fn seat_one_action(game: &Game, legal: &[Space]) -> Action {
    if game.public.players[1].supply.food < 4 && legal.contains(&Space::DayLaborer) {
        return place_day_laborer();
    }
    Action {
        space: first_plain_space(legal),
        detail: Detail::None,
    }
}

/// Drive the recorded game forward one legal placement, asserting that no
/// pending decision ever appears.
fn apply_recorded(game: &mut Game) {
    let legal = game.legal_spaces();
    assert!(!legal.is_empty(), "a legal placement must exist");
    let action = if game.public.active_player == 0 {
        seat_zero_action(game, &legal)
    } else {
        seat_one_action(game, &legal)
    };
    game.apply(action)
        .expect("scripted placement must be legal");
    assert!(
        !game.has_pending(),
        "B059 must not create a pending decision"
    );
}

fn drive_to_finished(game: &mut Game) {
    for _ in 0..200 {
        if game.public.finished {
            return;
        }
        apply_recorded(game);
    }
    panic!("recorded drive never reached the final harvest");
}

/// Real resume/replay proof: B059 is played from the explicitly dealt hand
/// through the Major Improvement action space with legal recorded actions
/// only. Save/loading and a full replay from setup, hands, start player,
/// and the action log must reproduce the exact state hash and food count,
/// so the space-dependent grant never fires twice.
#[test]
fn b059_major_improvement_grant_survives_resume_without_double_count() {
    let (config, set, hands, mut game) = game_with_b059();
    // The start player begins with the engine's initial food (2 per
    // `Player::new`). Rewriting the post-setup supply directly would not
    // be recorded in the action log and would desync the full replay, so
    // read the setup value and expect the grant on top of it. The script
    // only secures wood before B059 (a recorded Forest placement), so no
    // food changes before the play and the expectation is initial + 4.
    let initial_food = game.public.players[0].supply.food;

    // Drive until B059 is played and assert the one-time grant on the spot.
    let mut plays = 0;
    let mut wood_before_play = 0;
    while !game.public.players[0]
        .played_cards
        .iter()
        .any(|id| id == "B059")
    {
        assert!(plays < 200, "B059 was never played");
        plays += 1;
        assert!(
            !game.public.finished,
            "the game ended before B059 could be played"
        );
        wood_before_play = game.public.players[0].supply.wood;
        apply_recorded(&mut game);
    }
    let p = &game.public.players[0];
    assert_eq!(
        p.supply.food,
        initial_food + 4,
        "the Major Improvement action grants 4 food exactly once on top of the setup value"
    );
    assert_eq!(
        p.supply.wood,
        wood_before_play - 1,
        "cost wood 1 is paid from the wood secured by recorded placements"
    );
    let grant_food = p.supply.food;

    // Resume immediately after the grant: save/load and a full replay of
    // the action log recorded so far must reproduce the exact state hash
    // and the food count at the B059 play, proving the space-dependent
    // grant survives a resume without firing a second time.
    let grant_hash = game.state_hash();
    let grant_loaded = Game::load_json(&game.save_json().expect("save after grant"))
        .expect("post-grant game must resume");
    assert_eq!(
        grant_loaded.state_hash(),
        grant_hash,
        "resume right after the grant must reproduce the state hash"
    );
    assert_eq!(
        grant_loaded.public.players[0].supply.food, grant_food,
        "resume right after the grant must reproduce food"
    );
    assert!(grant_loaded.public.players[0]
        .played_cards
        .iter()
        .any(|id| id == "B059"));

    let grant_replayed = Game::replay_with_setup_and_start_player(
        config.clone(),
        set.clone(),
        Some(hands.clone()),
        Some(0),
        &game.log.events,
    )
    .expect("recorded B059 game must replay up to the grant");
    assert_eq!(
        grant_replayed.state_hash(),
        grant_hash,
        "replay up to the grant must reproduce the state hash"
    );
    assert_eq!(
        grant_replayed.public.players[0].supply.food, grant_food,
        "replay up to the grant must reproduce food"
    );
    assert!(grant_replayed.public.players[0]
        .played_cards
        .iter()
        .any(|id| id == "B059"));

    drive_to_finished(&mut game);
    assert!(game.public.finished, "drive must reach the final harvest");
    assert_eq!(game.public.round, 14);
    assert!(game.public.players[0]
        .played_cards
        .iter()
        .any(|id| id == "B059"));

    // Feedings after the grant consume food freely, so the final count is
    // not a fixed anchor. What proves no double grant is the exact state
    // hash / food reproduction across save/load and full replay below.
    let final_hash = game.state_hash();
    let final_food = game.public.players[0].supply.food;

    let loaded = Game::load_json(&game.save_json().expect("save recorded game"))
        .expect("saved game must resume");
    assert_eq!(
        loaded.state_hash(),
        final_hash,
        "resume must reproduce state hash"
    );
    assert_eq!(
        loaded.public.players[0].supply.food, final_food,
        "resume must reproduce food"
    );
    assert!(loaded.public.players[0]
        .played_cards
        .iter()
        .any(|id| id == "B059"));

    let replayed = Game::replay_with_setup_and_start_player(
        config,
        set,
        Some(hands),
        Some(0),
        &game.log.events,
    )
    .expect("recorded B059 game must replay");
    assert_eq!(
        replayed.state_hash(),
        final_hash,
        "replay must reproduce state hash"
    );
    assert_eq!(
        replayed.public.players[0].supply.food, final_food,
        "replay must reproduce food"
    );
    assert!(replayed.public.players[0]
        .played_cards
        .iter()
        .any(|id| id == "B059"));
}
