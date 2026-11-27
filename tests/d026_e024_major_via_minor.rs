//! Tests for D026 大工の敷地 (Carpenter's Yard) and E024 野心 (Ambition).
//!
//! D026: cost 1 wood + 1 reed, 1 VP. Passive: when taking a Minor Improvement
//! action (MeetingPlace), may build Joinery or Well from the improvement market.
//! E024: prerequisite 2 occupations, 0 VP. Passive: when taking a Minor Improvement
//! action (MeetingPlace), may build ANY major improvement from the improvement market.
//!
//! Both cards are Minor Improvements. The passive effect only applies when the
//! card is PLAYED (in the player's played_cards), not merely in hand.

use agricola_re_ai::cards::{verified_basic_card_set, CardKind, CardSet};
use agricola_re_ai::engine::{Action, Detail};
use agricola_re_ai::harvest::SimpleHarvestDetails;
use agricola_re_ai::model::{Config, Game, Improvement, RoundCard, Space};
use agricola_re_ai::registry::build_default_registry;

/// Helper to create a card set with D026 and E024 added.
fn set_with_d026_e024() -> CardSet {
    // D026 and E024 are part of verified_extra_simple_cards, which is part
    // of the verified basic card set.
    verified_basic_card_set(2).unwrap()
}

/// Add D026 to player 0's hand.
fn give_d026_to_hand(game: &mut Game) {
    for hand in game.cards.hands.iter_mut() {
        hand.retain(|id| id != "D026");
    }
    game.cards.undealt.retain(|id| id != "D026");
    game.cards.hands[0].push("D026".to_owned());
}

/// Add E024 to player 0's hand.
fn give_e024_to_hand(game: &mut Game) {
    for hand in game.cards.hands.iter_mut() {
        hand.retain(|id| id != "E024");
    }
    game.cards.undealt.retain(|id| id != "E024");
    game.cards.hands[0].push("E024".to_owned());
}

/// Put `occs` into player 0's played area, removing them from undealt so the
/// card location partition stays consistent (each card id exactly one zone).
fn put_occupations_in_played(game: &mut Game, occs: &[String]) {
    for id in occs {
        for hand in game.cards.hands.iter_mut() {
            hand.retain(|x| x != id);
        }
        game.cards.undealt.retain(|x| x != id);
    }
    game.public.players[0]
        .played_cards
        .extend(occs.iter().cloned());
}

/// Advance game to a state where MeetingPlace is available and player has resources.
fn setup_meeting_place_fixture() -> (Game, CardSet) {
    let config = Config {
        players: 2,
        seed: 42,
        additional_action_space_enabled: false,
    };
    let set = set_with_d026_e024();
    let mut game = Game::with_cards(config, set.clone()).unwrap();

    // Advance to round where MeetingPlace exists (round 1 has it)
    game.start_round().unwrap();

    // Ensure MeetingPlace is not used
    game.public.used.remove(&Space::MeetingPlace);

    // Give player 0 resources to build improvements
    game.public.players[0].supply.wood = 10;
    game.public.players[0].supply.reed = 10;
    game.public.players[0].supply.clay = 10;
    game.public.players[0].supply.stone = 10;
    game.public.players[0].supply.food = 10;

    // Make player 0 the active player
    game.public.active_player = 0;
    game.public.workers_placed = game.public.work_quota.clone();
    game.public.workers_placed[0] -= 1; // One worker available

    (game, set)
}

#[test]
fn d026_registry_reachable() {
    // D026 must be reachable via the default registry
    let registry = build_default_registry().expect("default registry must build");
    let card = registry
        .get_by_external("D026")
        .expect("D026 must be in registry");
    assert_eq!(card.external_id, "D026");
    assert_eq!(card.kind, CardKind::MinorImprovement);
    assert_eq!(card.points, 1);
    // Cost: 1 wood + 1 reed
    assert_eq!(
        card.cost,
        vec![
            (agricola_re_ai::model::Good::Wood, 1),
            (agricola_re_ai::model::Good::Reed, 1)
        ]
    );
}

#[test]
fn e024_registry_reachable() {
    // E024 must be reachable via the default registry
    let registry = build_default_registry().expect("default registry must build");
    let card = registry
        .get_by_external("E024")
        .expect("E024 must be in registry");
    assert_eq!(card.external_id, "E024");
    assert_eq!(card.kind, CardKind::MinorImprovement);
    assert_eq!(card.points, 0);
    // Prerequisite: MinimumOccupations(2)
    let has_prereq = card.effects.iter().any(|e| {
        matches!(e, agricola_re_ai::cards::CardEffect::Prerequisite { rule } if matches!(rule, agricola_re_ai::cards::CardRequirement::MinimumOccupations(2)))
    });
    assert!(
        has_prereq,
        "E024 must have MinimumOccupations(2) prerequisite"
    );
}

#[test]
fn d026_allows_joinery_and_well_on_meeting_place() {
    let (mut game, _set) = setup_meeting_place_fixture();

    // Play D026
    give_d026_to_hand(&mut game);
    let play_action = Action {
        space: Space::MeetingPlace,
        detail: Detail::PlayMinor {
            card_id: "D026".to_owned(),
        },
    };
    game.apply_with_harvest_policy(play_action, &SimpleHarvestDetails)
        .unwrap();

    // Verify D026 is in played cards
    assert!(game.public.players[0]
        .played_cards
        .contains(&"D026".to_owned()));

    // Now try to build Joinery via MeetingPlace (should be allowed)
    // First ensure Joinery is in the market
    assert!(game
        .public
        .improvement_market
        .contains(&Improvement::Joinery));

    // Try to buy Joinery on MeetingPlace
    let buy_joinery = Action {
        space: Space::MeetingPlace,
        detail: Detail::Buy {
            improvement: Improvement::Joinery,
            instant_bake: 0,
        },
    };
    let result = game.apply_with_harvest_policy(buy_joinery, &SimpleHarvestDetails);
    assert!(
        result.is_ok(),
        "D026 should allow buying Joinery on MeetingPlace: {:?}",
        result.err()
    );

    // Verify Joinery is now owned
    assert!(game.public.players[0]
        .improvements
        .contains(&Improvement::Joinery));
    assert!(!game
        .public
        .improvement_market
        .contains(&Improvement::Joinery));
}

#[test]
fn d026_allows_well_on_meeting_place() {
    let (mut game, _set) = setup_meeting_place_fixture();

    // Play D026
    give_d026_to_hand(&mut game);
    let play_action = Action {
        space: Space::MeetingPlace,
        detail: Detail::PlayMinor {
            card_id: "D026".to_owned(),
        },
    };
    game.apply_with_harvest_policy(play_action, &SimpleHarvestDetails)
        .unwrap();

    // Ensure Well is in the market
    assert!(game.public.improvement_market.contains(&Improvement::Well));

    // Try to buy Well on MeetingPlace
    let buy_well = Action {
        space: Space::MeetingPlace,
        detail: Detail::Buy {
            improvement: Improvement::Well,
            instant_bake: 0,
        },
    };
    let result = game.apply_with_harvest_policy(buy_well, &SimpleHarvestDetails);
    assert!(
        result.is_ok(),
        "D026 should allow buying Well on MeetingPlace: {:?}",
        result.err()
    );

    // Verify Well is now owned
    assert!(game.public.players[0]
        .improvements
        .contains(&Improvement::Well));
}

#[test]
fn d026_rejects_other_improvements_on_meeting_place() {
    let (mut game, _set) = setup_meeting_place_fixture();

    // Play D026
    give_d026_to_hand(&mut game);
    let play_action = Action {
        space: Space::MeetingPlace,
        detail: Detail::PlayMinor {
            card_id: "D026".to_owned(),
        },
    };
    game.apply_with_harvest_policy(play_action, &SimpleHarvestDetails)
        .unwrap();

    // Try to buy ClayOven (not allowed by D026)
    let buy_clay_oven = Action {
        space: Space::MeetingPlace,
        detail: Detail::Buy {
            improvement: Improvement::ClayOven,
            instant_bake: 0,
        },
    };
    let result = game.apply_with_harvest_policy(buy_clay_oven, &SimpleHarvestDetails);
    assert!(
        result.is_err(),
        "D026 should reject ClayOven on MeetingPlace"
    );
    assert!(result.unwrap_err().contains("not allowed"));
}

#[test]
fn d026_without_card_played_rejects_major_on_meeting_place() {
    let (mut game, _set) = setup_meeting_place_fixture();

    // Do NOT play D026, just have it in hand
    give_d026_to_hand(&mut game);

    // Try to buy Joinery on MeetingPlace without playing D026
    let buy_joinery = Action {
        space: Space::MeetingPlace,
        detail: Detail::Buy {
            improvement: Improvement::Joinery,
            instant_bake: 0,
        },
    };
    let result = game.apply_with_harvest_policy(buy_joinery, &SimpleHarvestDetails);
    assert!(
        result.is_err(),
        "Without D026 played, major improvements should be rejected on MeetingPlace"
    );
    assert!(result.unwrap_err().contains("not allowed"));
}

#[test]
fn e024_allows_any_major_on_meeting_place() {
    let (mut game, set) = setup_meeting_place_fixture();

    // Give player 2 occupations to satisfy E024 prerequisite
    let occs: Vec<String> = set
        .definitions
        .iter()
        .filter(|c| c.kind == CardKind::Occupation)
        .map(|c| c.id.clone())
        .take(2)
        .collect();
    assert_eq!(occs.len(), 2, "two occupations must exist in the card set");
    put_occupations_in_played(&mut game, &occs);

    // Play E024
    give_e024_to_hand(&mut game);
    let play_action = Action {
        space: Space::MeetingPlace,
        detail: Detail::PlayMinor {
            card_id: "E024".to_owned(),
        },
    };
    game.apply_with_harvest_policy(play_action, &SimpleHarvestDetails)
        .unwrap();

    // Verify E024 is in played cards
    assert!(game.public.players[0]
        .played_cards
        .contains(&"E024".to_owned()));

    // Try to buy various major improvements
    for imp in [
        Improvement::Joinery,
        Improvement::Well,
        Improvement::ClayOven,
        Improvement::StoneOven,
        Improvement::Pottery,
        Improvement::Basketry,
    ] {
        if game.public.improvement_market.contains(&imp) {
            let buy_action = Action {
                space: Space::MeetingPlace,
                detail: Detail::Buy {
                    improvement: imp,
                    instant_bake: 0,
                },
            };
            let result = game.apply_with_harvest_policy(buy_action, &SimpleHarvestDetails);
            assert!(
                result.is_ok(),
                "E024 should allow buying {:?} on MeetingPlace: {:?}",
                imp,
                result.err()
            );

            // Verify improvement is now owned
            assert!(game.public.players[0].improvements.contains(&imp));
            break; // Just test one since market changes after purchase
        }
    }
}

#[test]
fn e024_requires_two_occupations() {
    let (mut game, set) = setup_meeting_place_fixture();

    // Give player only 1 occupation
    let occs: Vec<String> = set
        .definitions
        .iter()
        .filter(|c| c.kind == CardKind::Occupation)
        .map(|c| c.id.clone())
        .take(1)
        .collect();
    assert_eq!(occs.len(), 1);
    put_occupations_in_played(&mut game, &occs);

    // Try to play E024 (should fail due to prerequisite)
    give_e024_to_hand(&mut game);
    let play_action = Action {
        space: Space::MeetingPlace,
        detail: Detail::PlayMinor {
            card_id: "E024".to_owned(),
        },
    };
    let result = game.apply_with_harvest_policy(play_action, &SimpleHarvestDetails);
    assert!(result.is_err(), "E024 should require 2 occupations to play");
}

#[test]
fn e024_without_card_played_rejects_major_on_meeting_place() {
    let (mut game, set) = setup_meeting_place_fixture();

    // Give player 2 occupations
    let occs: Vec<String> = set
        .definitions
        .iter()
        .filter(|c| c.kind == CardKind::Occupation)
        .map(|c| c.id.clone())
        .take(2)
        .collect();
    assert_eq!(occs.len(), 2);
    put_occupations_in_played(&mut game, &occs);

    // Do NOT play E024, just have it in hand
    give_e024_to_hand(&mut game);

    // Try to buy Joinery on MeetingPlace without playing E024
    let buy_joinery = Action {
        space: Space::MeetingPlace,
        detail: Detail::Buy {
            improvement: Improvement::Joinery,
            instant_bake: 0,
        },
    };
    let result = game.apply_with_harvest_policy(buy_joinery, &SimpleHarvestDetails);
    assert!(
        result.is_err(),
        "Without E024 played, major improvements should be rejected on MeetingPlace"
    );
}

#[test]
fn major_improvement_space_still_works_normally() {
    // Verify that the MajorImprovement space still works normally for buying improvements
    let (mut game, _set) = setup_meeting_place_fixture();

    // Ensure Joinery is in the market
    assert!(game
        .public
        .improvement_market
        .contains(&Improvement::Joinery));

    // MajorImprovement must be revealed before the space can be taken
    game.public.revealed.push(RoundCard::MajorImprovement);
    game.hidden_round_deck
        .retain(|c| *c != RoundCard::MajorImprovement);
    game.public.round = game.public.revealed.len() as u8;

    // Buy Joinery on MajorImprovement space (normal path)
    let buy_joinery = Action {
        space: Space::MajorImprovement,
        detail: Detail::Buy {
            improvement: Improvement::Joinery,
            instant_bake: 0,
        },
    };
    let result = game.apply_with_harvest_policy(buy_joinery, &SimpleHarvestDetails);
    assert!(
        result.is_ok(),
        "MajorImprovement space should still allow buying Joinery: {:?}",
        result.err()
    );

    // Verify Joinery is now owned
    assert!(game.public.players[0]
        .improvements
        .contains(&Improvement::Joinery));
}

#[test]
fn play_minor_on_meeting_place_still_works() {
    // Verify that playing minor improvements on MeetingPlace still works normally
    let (mut game, _set) = setup_meeting_place_fixture();

    // Add a simple minor improvement to hand (B010 - Housing Space)
    for hand in game.cards.hands.iter_mut() {
        hand.retain(|id| id != "B010");
    }
    game.cards.undealt.retain(|id| id != "B010");
    game.cards.hands[0].push("B010".to_owned());

    // Play B010 on MeetingPlace
    let play_action = Action {
        space: Space::MeetingPlace,
        detail: Detail::PlayMinor {
            card_id: "B010".to_owned(),
        },
    };
    let result = game.apply_with_harvest_policy(play_action, &SimpleHarvestDetails);
    assert!(
        result.is_ok(),
        "Playing minor improvements on MeetingPlace should still work: {:?}",
        result.err()
    );

    // Verify B010 is in played cards
    assert!(game.public.players[0]
        .played_cards
        .contains(&"B010".to_owned()));
}

#[test]
fn save_load_replay_preserves_major_via_minor_effect() {
    // E024 野心: prerequisite 2 occupations; passive lets the owner build any
    // major improvement with a Minor Improvement (MeetingPlace) action.
    // Fireplace2 costs clay 2 (no stone), so the whole flow can be driven from
    // a clean, fully logged game state that replay reproduces exactly from
    // construction + the logged events. D026's permission (Joinery/Well) would
    // require stone from revealed quarries, which is not replay-friendly.
    let config = Config {
        players: 2,
        seed: 42,
        additional_action_space_enabled: false,
    };
    let set = set_with_d026_e024();

    // Draft hands: 7 occupations + 7 minors per seat (from_hands requirement).
    // Occupations used for Lessons plays must be free; E024 itself is free.
    let free_occs: Vec<String> = set
        .definitions
        .iter()
        .filter(|c| c.kind == CardKind::Occupation && c.cost.is_empty())
        .map(|c| c.id.clone())
        .collect();
    assert!(
        free_occs.len() >= 14,
        "need 14 free occupations for the two draft hands"
    );
    let minors: Vec<String> = set
        .definitions
        .iter()
        .filter(|c| c.kind == CardKind::MinorImprovement && c.id != "E024")
        .map(|c| c.id.clone())
        .collect();
    assert!(
        minors.len() >= 13,
        "need 13 non-E024 minors for the two draft hands"
    );
    let hands = vec![
        free_occs[0..7]
            .iter()
            .cloned()
            .chain(std::iter::once("E024".to_owned()))
            .chain(minors[0..6].iter().cloned())
            .collect(),
        free_occs[7..14]
            .iter()
            .cloned()
            .chain(minors[6..13].iter().cloned())
            .collect(),
    ];

    let mut game =
        Game::with_cards_and_hands_with_start_player(config.clone(), set.clone(), hands.clone(), 0)
            .unwrap();

    // Play a free occupation via Lessons (first occupation costs no fee).
    fn play_occupation(game: &mut Game, card_id: &str) {
        let actor = game.public.active_player;
        let result = game.apply(Action {
            space: Space::Lessons,
            detail: Detail::PlayOccupation {
                card_id: card_id.to_owned(),
            },
        });
        assert!(
            result.is_ok(),
            "actor {actor} Lessons PlayOccupation {card_id} must be legal: {result:?}"
        );
    }

    // Take a resource space with the active player (each space once per round).
    fn take_space(game: &mut Game, preferred: &[Space]) {
        let actor = game.public.active_player;
        let legal = game.legal_spaces();
        let choice = preferred
            .iter()
            .find(|s| legal.contains(s))
            .copied()
            .or_else(|| legal.first().copied())
            .expect("a legal space must exist");
        let result = game.apply(Action {
            space: choice,
            detail: Detail::None,
        });
        assert!(
            result.is_ok(),
            "actor {actor} {choice:?} must be legal: {result:?}"
        );
    }

    let seat1_pref = &[Space::DayLaborer, Space::Fishing, Space::ClayPit];

    // Round 1: seat 0 plays occupation #1 (Lessons), then takes clay.
    play_occupation(&mut game, &free_occs[0]);
    take_space(&mut game, seat1_pref); // seat 1 filler
    take_space(&mut game, &[Space::ClayPit]); // seat 0: clay 1
    take_space(&mut game, seat1_pref); // seat 1 filler -> round 2

    // Round 2: seat 0 plays occupation #2 (E024 prerequisite now met), then clay again.
    play_occupation(&mut game, &free_occs[1]);
    take_space(&mut game, seat1_pref); // seat 1 filler
    take_space(&mut game, &[Space::ClayPit]); // seat 0: clay 2
    take_space(&mut game, seat1_pref); // seat 1 filler -> round 3

    // Round 3: seat 0 plays E024 on MeetingPlace.
    let play_e024 = Action {
        space: Space::MeetingPlace,
        detail: Detail::PlayMinor {
            card_id: "E024".to_owned(),
        },
    };
    let result = game.apply(play_e024);
    assert!(
        result.is_ok(),
        "Playing E024 on MeetingPlace must be legal: {result:?}"
    );
    assert!(
        game.public.players[0]
            .played_cards
            .contains(&"E024".to_owned()),
        "E024 must be in played cards"
    );
    take_space(&mut game, seat1_pref); // seat 1 filler
    take_space(&mut game, &[Space::Forest]); // seat 0 second worker -> round 4
    take_space(&mut game, seat1_pref); // seat 1 second worker

    // Seat 0 now has clay 2 and E024 played: Fireplace2 (clay 2) must be
    // buyable on MeetingPlace via the E024 passive.
    let hash_pre_buy = game.state_hash();
    let buy_fireplace = Action {
        space: Space::MeetingPlace,
        detail: Detail::Buy {
            improvement: Improvement::Fireplace2,
            instant_bake: 0,
        },
    };
    let result = game.apply_with_harvest_policy(buy_fireplace.clone(), &SimpleHarvestDetails);
    assert!(
        result.is_ok(),
        "E024 must allow buying Fireplace2 on MeetingPlace: {result:?}"
    );
    assert!(
        game.public.players[0]
            .improvements
            .contains(&Improvement::Fireplace2),
        "Fireplace2 must be owned"
    );

    // --- save/load round trip preserves the full state including the effect ---
    let hash_post_buy = game.state_hash();
    let loaded =
        Game::load_json(&game.save_json().expect("save must succeed")).expect("load must succeed");
    assert_eq!(loaded.state_hash(), hash_post_buy);
    assert!(
        loaded.public.players[0]
            .played_cards
            .contains(&"E024".to_owned()),
        "loaded game must keep E024 played"
    );

    // --- replay: first 12 events reproduce the state right before the buy ---
    let events = game.log.events.clone();
    let pre_buy_events = &events[..events.len() - 1];
    let mut replayed = Game::replay_with_setup_and_start_player(
        config.clone(),
        set.clone(),
        Some(hands.clone()),
        Some(0),
        pre_buy_events,
    )
    .expect("replay must succeed");
    assert_eq!(
        replayed.state_hash(),
        hash_pre_buy,
        "replay must reproduce the pre-buy state hash"
    );
    assert!(
        replayed.public.players[0]
            .played_cards
            .contains(&"E024".to_owned()),
        "replayed game must keep E024 played"
    );

    // The replayed game must preserve the E024 permission: Fireplace2 buyable.
    let result = replayed.apply_with_harvest_policy(buy_fireplace, &SimpleHarvestDetails);
    assert!(
        result.is_ok(),
        "replayed game must allow buying Fireplace2 via E024: {result:?}"
    );
}

#[test]
fn fireplace_hearth_exchange_still_works() {
    // Verify that Fireplace -> Hearth exchange on MajorImprovement space still works
    let (mut game, _set) = setup_meeting_place_fixture();

    // Give player a Fireplace (remove from market so each major exists once)
    game.public
        .improvement_market
        .retain(|&i| i != Improvement::Fireplace2);
    game.public.players[0]
        .improvements
        .push(Improvement::Fireplace2);
    game.public.players[0].supply.clay = 10;

    // Ensure Hearth4 is in the market
    assert!(game
        .public
        .improvement_market
        .contains(&Improvement::Hearth4));

    // MajorImprovement must be revealed before the space can be taken
    game.public.revealed.push(RoundCard::MajorImprovement);
    game.hidden_round_deck
        .retain(|c| *c != RoundCard::MajorImprovement);
    game.public.round = game.public.revealed.len() as u8;

    // Buy Hearth4 on MajorImprovement space (should exchange Fireplace2)
    let buy_hearth = Action {
        space: Space::MajorImprovement,
        detail: Detail::Buy {
            improvement: Improvement::Hearth4,
            instant_bake: 0,
        },
    };
    let result = game.apply_with_harvest_policy(buy_hearth, &SimpleHarvestDetails);
    assert!(
        result.is_ok(),
        "Fireplace->Hearth exchange should still work: {:?}",
        result.err()
    );

    // Verify Hearth4 is owned, Fireplace2 is gone
    assert!(game.public.players[0]
        .improvements
        .contains(&Improvement::Hearth4));
    assert!(!game.public.players[0]
        .improvements
        .contains(&Improvement::Fireplace2));
}
