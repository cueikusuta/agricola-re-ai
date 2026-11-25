//! Tests for E077 根掘り鍬 (Mattock) through the real engine paths.
//!
//! The band pools are deliberately kept out of the A/B two-player preset, so
//! the focused fixtures append the E077 definition to a verified-preset
//! sandbox and relocate the card (the same pattern `tests/b058_crack_weeder.rs`
//! uses); the replay fixture deals E077 into an explicit hand instead, so its
//! action log is sufficient to reconstruct the game.
//!
//! Covered per the spec test contract (data/bga-card-tests/E077.json):
//! - registry reachability (E077 resolves via `build_default_registry`)
//! - E077-reed-space / E077-stone-space: each reed/stone action space use
//!   grants exactly 1 clay (ReedBank, WesternQuarry, EasternQuarry and — its
//!   core action supplies stone/reed too — ResourceMarket)
//! - E077-other-space: wood/food/grain spaces grant nothing
//! - a non-owner's use of a trigger space grants the owner nothing
//! - dispatcher: `dispatch_after_worker` owner/non-owner/unrelated per space
//! - resume/replay: a game that plays E077 and uses trigger spaces entirely
//!   through recorded legal actions; save/load and full setup/hands/
//!   action-log replay reproduce the exact state hash and clay count without
//!   granting the effect twice

use agricola_re_ai::card_events::dispatch_after_worker;
use agricola_re_ai::cards::{verified_band_e_cards, CardDefinition, CardKind, CardSet};
use agricola_re_ai::engine::{Action, Detail};
use agricola_re_ai::model::{Config, Game, Good, RoundCard, Space};
use agricola_re_ai::pending::Transition;
use agricola_re_ai::registry::build_default_registry;

/// The engine's core-action spaces that supply reed or stone (E077's triggers).
const TRIGGER_SPACES: [Space; 4] = [
    Space::ReedBank,
    Space::WesternQuarry,
    Space::EasternQuarry,
    Space::ResourceMarket,
];

/// Detail-free spaces that supply neither reed nor stone.
const PLAIN_SPACES: [Space; 5] = [
    Space::Forest,
    Space::Fishing,
    Space::DayLaborer,
    Space::GrainSeeds,
    Space::VegetableSeeds,
];

fn e077_definition() -> CardDefinition {
    verified_band_e_cards()
        .into_iter()
        .find(|c| c.id == "E077")
        .expect("E077 must be defined in verified_band_e_cards")
}

/// Verified two-player preset plus the E077 definition appended.
fn set_with_e077() -> CardSet {
    let mut set = agricola_re_ai::cards::verified_basic_card_set(2).unwrap();
    set.definitions.push(e077_definition());
    set
}

/// Move E077 to exactly one location (a hand, or a played area for
/// `into_play`), keeping the "every implemented card occupies exactly one
/// zone" partition valid.
fn relocate_e077(game: &mut Game, into_play: bool) {
    for hand in game.cards.hands.iter_mut() {
        hand.retain(|id| id != "E077");
    }
    game.cards.undealt.retain(|id| id != "E077");
    if into_play {
        game.public.players[0].played_cards.push("E077".to_owned());
    } else {
        game.cards.hands[0].push("E077".to_owned());
    }
}

fn plain(space: Space) -> Action {
    Action {
        space,
        detail: Detail::None,
    }
}

fn first_plain_space(legal: &[Space]) -> Space {
    PLAIN_SPACES
        .iter()
        .copied()
        .find(|space| legal.contains(space))
        .expect("at least one detail-free non-trigger space must be legal")
}

/// One isolated placement on the teleport fixture: turn order is not modelled,
/// so the used-space ledger and worker counters are cleared first.
fn take(game: &mut Game, action: Action) {
    game.public.active_player = 0;
    game.public.used.clear();
    game.public.workers_placed = vec![0; game.public.players.len()];
    game.apply(action).expect("placement must be legal");
}

/// A two-player sandbox advanced by the real `start_round` path until every
/// space the focused tests use is on the board: the Major Improvement space
/// (stage 1) and both quarries (stages 2 and 4). E077 waits in seat 0's hand
/// with its 1-wood cost pre-paid-ready, and the additional-space setting is on
/// so ResourceMarket is usable at two players.
fn focused_game() -> Game {
    let config = Config {
        players: 2,
        seed: 7,
        additional_action_space_enabled: true,
    };
    let mut game = Game::with_cards(config, set_with_e077()).unwrap();
    while !game.public.revealed.contains(&RoundCard::MajorImprovement)
        || !game.public.revealed.contains(&RoundCard::WesternQuarry)
        || !game.public.revealed.contains(&RoundCard::EasternQuarry)
    {
        game.start_round().unwrap();
    }
    relocate_e077(&mut game, false);
    game.public.active_player = 0;
    game.public.players[0].supply.wood = 1;
    game
}

#[test]
fn e077_reachable_via_default_registry() {
    let registry = build_default_registry().expect("default registry builds");
    let card_id = registry
        .card_id("E077")
        .expect("E077 must be registered (band E pool)");
    let definition = registry
        .get(card_id)
        .expect("registered E077 resolves")
        .to_card_definition();
    assert_eq!(definition.kind, CardKind::MinorImprovement);
    assert_eq!(definition.cost, vec![(Good::Wood, 1)]);
    assert_eq!(definition.points, 0);
    assert_eq!(definition.effects.len(), 4, "one AfterWorkerGain per space");
    assert!(definition.effects.iter().all(|effect| matches!(
        effect,
        agricola_re_ai::cards::CardEffect::AfterWorkerGain {
            good: Good::Clay,
            amount: 1,
            ..
        }
    )));
}

/// E077-reed-space / E077-stone-space / E077-other-space: the engine play path
/// (MajorImprovement + PlayMinor, the "Minor Improvement" action) pays the
/// wood 1 cost, and each listed action space then grants exactly 1 clay while
/// wood/food/grain spaces grant nothing.
#[test]
fn e077_play_then_each_action_space_grants_one_clay() {
    let mut game = focused_game();

    game.apply(Action {
        space: Space::MajorImprovement,
        detail: Detail::PlayMinor {
            card_id: "E077".to_owned(),
        },
    })
    .expect("E077 play via the Minor Improvement action");

    let p = &game.public.players[0];
    assert_eq!(p.supply.wood, 0, "cost wood 1 is paid");
    assert_eq!(p.supply.clay, 0, "playing E077 grants no clay itself");
    assert!(p.played_cards.iter().any(|id| id == "E077"));
    assert!(!game.cards.hands[0].iter().any(|id| id == "E077"));

    // Each reed/stone action space use grants exactly one clay.
    for space in TRIGGER_SPACES {
        let before = game.public.players[0].supply.clay;
        take(&mut game, plain(space));
        assert_eq!(
            game.public.players[0].supply.clay,
            before + 1,
            "E077 must grant exactly 1 clay on {space:?}"
        );
    }
    // A second use of the same space fires again (1 use = 1 grant), so the
    // count above covers every trigger exactly once per use.
    let before = game.public.players[0].supply.clay;
    take(&mut game, plain(Space::ReedBank));
    assert_eq!(game.public.players[0].supply.clay, before + 1);

    // Non-trigger spaces grant nothing.
    for space in PLAIN_SPACES {
        let before = game.public.players[0].supply.clay;
        take(&mut game, plain(space));
        assert_eq!(
            game.public.players[0].supply.clay, before,
            "E077 must not fire on {space:?}"
        );
    }
}

/// A non-owner's use of a trigger space grants the E077 owner nothing.
#[test]
fn e077_does_not_fire_for_other_players() {
    let mut game = focused_game();
    take(
        &mut game,
        Action {
            space: Space::MajorImprovement,
            detail: Detail::PlayMinor {
                card_id: "E077".to_owned(),
            },
        },
    );
    let owner_clay = game.public.players[0].supply.clay;
    assert_eq!(owner_clay, 0);

    // Seat 1 (no E077) uses the reed bank and the western quarry.
    game.public.active_player = 1;
    game.public.used.clear();
    game.public.workers_placed = vec![0; game.public.players.len()];
    game.apply(plain(Space::ReedBank))
        .expect("seat 1 may use the reed bank");
    game.public.active_player = 1;
    game.public.used.clear();
    game.public.workers_placed = vec![0; game.public.players.len()];
    game.apply(plain(Space::WesternQuarry))
        .expect("seat 1 may use the western quarry");

    assert_eq!(
        game.public.players[0].supply.clay, owner_clay,
        "a non-owner's reed/stone use must not grant the owner clay"
    );
    assert_eq!(
        game.public.players[1].supply.clay, 0,
        "a non-owner never holds E077 and gains no clay"
    );
}

/// Dispatcher contract: `dispatch_after_worker` grants 1 clay to the owner on
/// each trigger space, nothing to a non-owner, and nothing on unrelated spaces.
#[test]
fn e077_dispatcher_owner_non_owner_and_unrelated_space() {
    let mut game = focused_game();
    relocate_e077(&mut game, true);

    let owner = 0usize;
    let non_owner = 1usize;
    for space in TRIGGER_SPACES {
        // Owner: exactly one clay per use.
        let before = game.public.players[owner].supply.clay;
        let transition = dispatch_after_worker(
            &game.card_set,
            owner,
            space,
            &mut game.cards,
            &mut game.public.players,
            &mut game.public.improvement_market,
            &mut game.delayed_well,
            game.public.round,
            &game.public.used,
            &game.public.accumulations,
            &mut game.event_budget,
            &mut game.public.card_uses,
            &game.card_use_limits,
            &mut game.pending,
            false,
        )
        .expect("dispatch must succeed");
        assert!(matches!(transition, Transition::Completed));
        assert_eq!(
            game.public.players[owner].supply.clay,
            before + 1,
            "owner must gain 1 clay on {space:?}"
        );

        // Non-owner: nothing.
        let owner_before = game.public.players[owner].supply.clay;
        let transition = dispatch_after_worker(
            &game.card_set,
            non_owner,
            space,
            &mut game.cards,
            &mut game.public.players,
            &mut game.public.improvement_market,
            &mut game.delayed_well,
            game.public.round,
            &game.public.used,
            &game.public.accumulations,
            &mut game.event_budget,
            &mut game.public.card_uses,
            &game.card_use_limits,
            &mut game.pending,
            false,
        )
        .expect("dispatch must succeed");
        assert!(matches!(transition, Transition::Completed));
        assert_eq!(
            game.public.players[owner].supply.clay, owner_before,
            "a non-owner use must not change the owner's clay on {space:?}"
        );
        assert_eq!(
            game.public.players[non_owner].supply.clay, 0,
            "a non-owner never holds E077"
        );
    }

    // Owner on an unrelated space: nothing.
    let before = game.public.players[owner].supply.clay;
    let transition = dispatch_after_worker(
        &game.card_set,
        owner,
        Space::Forest,
        &mut game.cards,
        &mut game.public.players,
        &mut game.public.improvement_market,
        &mut game.delayed_well,
        game.public.round,
        &game.public.used,
        &game.public.accumulations,
        &mut game.event_budget,
        &mut game.public.card_uses,
        &game.card_use_limits,
        &mut game.pending,
        false,
    )
    .expect("dispatch must succeed");
    assert!(matches!(transition, Transition::Completed));
    assert_eq!(
        game.public.players[owner].supply.clay, before,
        "Forest must not trigger E077"
    );
}

/// Explicit initial hands put E077 in seat 0's hand without changing any
/// post-setup state outside the action log.
fn hands_with_e077(set: &CardSet) -> Vec<Vec<String>> {
    let occupations: Vec<String> = set
        .definitions
        .iter()
        .filter(|card| card.kind == CardKind::Occupation)
        .map(|card| card.id.clone())
        .collect();
    let minors: Vec<String> = set
        .definitions
        .iter()
        .filter(|card| card.kind == CardKind::MinorImprovement && card.id != "E077")
        .map(|card| card.id.clone())
        .collect();
    assert!(occupations.len() >= 14 && minors.len() >= 13);

    let mut seat_zero = occupations[..7].to_vec();
    seat_zero.push("E077".to_owned());
    seat_zero.extend_from_slice(&minors[..6]);
    let seat_one = [occupations[7..14].to_vec(), minors[6..13].to_vec()].concat();
    vec![seat_zero, seat_one]
}

/// Seat 0's scripted single placement. One action per call: every `apply` is a
/// recorded event, so the action log reconstructs the game exactly.
fn seat_zero_action(game: &Game, legal: &[Space]) -> Action {
    let player = &game.public.players[0];
    let e077_played = player.played_cards.iter().any(|id| id == "E077");
    let trigger_free = TRIGGER_SPACES
        .iter()
        .copied()
        .find(|space| legal.contains(space) && !game.public.used.contains(space));

    // Secure the 1 wood cost before E077 is played.
    if !e077_played && player.supply.wood < 1 && legal.contains(&Space::Forest) {
        return plain(Space::Forest);
    }

    // Play E077 through the Minor Improvement action once it is on the board.
    if !e077_played && legal.contains(&Space::MajorImprovement) {
        return Action {
            space: Space::MajorImprovement,
            detail: Detail::PlayMinor {
                card_id: "E077".to_owned(),
            },
        };
    }

    // After the play: one reed/stone action space per placement while food
    // stays above the 4-food feeding cost (2 family members).
    if e077_played && player.supply.food >= 5 {
        if let Some(space) = trigger_free {
            return plain(space);
        }
    }

    // Keep food at or above the feeding cost.
    if player.supply.food < 5 && legal.contains(&Space::DayLaborer) {
        return plain(Space::DayLaborer);
    }
    if player.supply.food < 5 && legal.contains(&Space::Fishing) {
        return plain(Space::Fishing);
    }

    // After the play, any still-free trigger space is preferred over filler.
    if e077_played {
        if let Some(space) = trigger_free {
            return plain(space);
        }
    }

    plain(first_plain_space(legal))
}

fn seat_one_action(game: &Game, legal: &[Space]) -> Action {
    if game.public.players[1].supply.food < 5 && legal.contains(&Space::DayLaborer) {
        return plain(Space::DayLaborer);
    }
    if game.public.players[1].supply.food < 5 && legal.contains(&Space::Fishing) {
        return plain(Space::Fishing);
    }
    plain(first_plain_space(legal))
}

/// Drive the recorded game to the final harvest, asserting on every seat-0
/// placement that a trigger space grants exactly one clay and nothing else
/// moves the owner's clay. Returns the number of trigger firings.
fn drive_to_finished(game: &mut Game) -> u32 {
    let mut fires = 0u32;
    for _ in 0..400 {
        if game.public.finished {
            return fires;
        }
        let legal = game.legal_spaces();
        assert!(!legal.is_empty(), "a legal placement must exist");
        let action = if game.public.active_player == 0 {
            seat_zero_action(game, &legal)
        } else {
            seat_one_action(game, &legal)
        };
        let e077_played = game.public.players[0]
            .played_cards
            .iter()
            .any(|id| id == "E077");
        let actor = game.public.active_player;
        let is_trigger = e077_played && actor == 0 && TRIGGER_SPACES.contains(&action.space);
        let clay_before = game.public.players[0].supply.clay;
        let placed_space = action.space;
        game.apply(action)
            .expect("scripted placement must be legal");
        assert!(
            !game.has_pending(),
            "E077 must not create a pending decision"
        );
        if is_trigger {
            assert_eq!(
                game.public.players[0].supply.clay,
                clay_before + 1,
                "E077 must grant exactly 1 clay on {:?}",
                placed_space
            );
            fires += 1;
        } else if actor == 0 {
            // A non-trigger seat-0 placement must not move the owner's clay.
            assert_eq!(
                game.public.players[0].supply.clay, clay_before,
                "a non-trigger placement must not grant clay on {:?}",
                placed_space
            );
        }
        assert_eq!(
            game.public.players[1].supply.clay, 0,
            "seat 1 never holds E077 and uses no clay space"
        );
    }
    panic!("recorded drive never reached the final harvest");
}

/// Real resume/replay proof: E077 is played from the explicitly dealt hand and
/// its trigger spaces are used solely through legal recorded actions.
/// Save/loading and a full replay from setup, hands, start player and the
/// action log must reproduce the identical state hash and clay count, proving
/// the bonus fires exactly once per use and never doubles on resume/replay.
#[test]
fn e077_clay_grant_survives_resume_without_double_count() {
    let config = Config {
        players: 2,
        seed: 2,
        additional_action_space_enabled: true,
    };
    let set = set_with_e077();
    let hands = hands_with_e077(&set);
    let mut game =
        Game::with_cards_and_hands_with_start_player(config.clone(), set.clone(), hands.clone(), 0)
            .expect("game setup with E077 in seat 0's hand");

    let fires = drive_to_finished(&mut game);
    assert!(game.public.finished, "drive must reach the final harvest");
    assert_eq!(game.public.round, 14);
    assert!(game.public.players[0]
        .played_cards
        .iter()
        .any(|id| id == "E077"));
    assert!(
        fires >= 4,
        "the drive must use every trigger space at least once: fires={fires}"
    );

    let final_hash = game.state_hash();
    let final_clay = game.public.players[0].supply.clay;
    assert_eq!(
        final_clay, fires,
        "the owner's clay equals exactly one grant per trigger use"
    );
    assert_eq!(game.public.players[1].supply.clay, 0);

    let loaded = Game::load_json(&game.save_json().expect("save recorded game"))
        .expect("saved E077 game must resume");
    assert_eq!(
        loaded.state_hash(),
        final_hash,
        "resume must reproduce state hash"
    );
    assert_eq!(
        loaded.public.players[0].supply.clay, final_clay,
        "resume must reproduce clay"
    );

    let replayed = Game::replay_with_setup_and_start_player(
        config,
        set,
        Some(hands),
        Some(0),
        &game.log.events,
    )
    .expect("recorded E077 game must replay");
    assert_eq!(
        replayed.state_hash(),
        final_hash,
        "replay must reproduce state hash"
    );
    assert_eq!(
        replayed.public.players[0].supply.clay, final_clay,
        "replay must reproduce clay"
    );
    assert_eq!(replayed.public.players[1].supply.clay, 0);
}
