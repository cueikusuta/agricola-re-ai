//! Engine-level acceptance for the first cards that use the earned-bonus-point
//! store: E098 神童 (Prodigy) and E101 はみだし者 (Outsider).
//!
//! Both are played through the real engine (Lessons space), so this covers the
//! production path `apply -> play_validate_pay -> play_from_hand ->
//! dispatch_play -> update_card_points_on_play` plus scoring, save/load and the
//! E101 occupation ban. The three-path (cards.rs / dispatcher / legacy)
//! comparison lives next to the dispatcher in `src/card_events.rs`.

use agricola_re_ai::cards::{
    can_play, verified_band_e_cards, verified_basic_card_set, CardKind, CardSet,
};
use agricola_re_ai::engine::{Action, Detail};
use agricola_re_ai::model::{Config, Game, Space};
use agricola_re_ai::policy::{CardChoiceDetails, DetailPolicy};

/// Two-player game whose seat 0 holds E098 and E101 plus five other
/// occupations and seven minors; seat 1 holds another 7 + 7. Remaining cards
/// stay undealt, so `CardSet`'s "one location per card" invariant holds.
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

    let mut seat0 = vec!["E098".to_string(), "E101".to_string()];
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
    assert_eq!(seat0.len(), 7);
    assert_eq!(seat1.len(), 7);
    // B061 is a free minor improvement, so seat 0 can play it with
    // MeetingPlace and own one improvement before E098.
    let mut seat0_minors = vec!["B061".to_string()];
    seat0_minors.extend(
        minors
            .iter()
            .filter(|id| id.as_str() != "B061")
            .take(6)
            .cloned(),
    );
    let seat1_minors: Vec<String> = minors
        .iter()
        .filter(|id| !seat0_minors.contains(id))
        .take(7)
        .cloned()
        .collect();
    assert_eq!(seat0_minors.len(), 7);
    assert_eq!(seat1_minors.len(), 7);
    let hands = vec![
        [seat0, seat0_minors].concat(),
        [seat1, seat1_minors].concat(),
    ];

    let game = Game::with_cards_and_hands_with_start_player(
        Config {
            players: 2,
            seed: 4242,
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

fn play_minor(game: &mut Game, card_id: &str) {
    game.apply(Action {
        space: Space::MeetingPlace,
        detail: Detail::PlayMinor {
            card_id: card_id.to_owned(),
        },
    })
    .unwrap();
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

/// Play legal actions (never an occupation) until seat 0 is active again while
/// Lessons is still unused. Round 1→2→3 contain no harvest, so this only
/// exercises ordinary worker placement for the other seats.
fn advance_to_seat0_with_lessons_open(game: &mut Game) {
    let mut guard = 0;
    while !(game.public.active_player == 0 && !game.public.used.contains(&Space::Lessons)) {
        assert!(
            !game.public.finished,
            "the game must not end during the wait"
        );
        let obs = game
            .observation(game.public.active_player)
            .expect("active seat observation");
        let space = game
            .legal_spaces()
            .into_iter()
            .find(|space| !matches!(space, Space::Lessons | Space::ExtraLessons))
            .expect("a non-occupation space");
        let action = CardChoiceDetails.produce(&obs, space).expect("detail");
        game.apply(action).expect("legal action applies");
        guard += 1;
        assert!(
            guard < 24,
            "failed to reach seat 0 with Lessons open (round {})",
            game.public.round
        );
    }
}

#[test]
fn e098_and_e101_store_play_time_bonus_points_and_reach_the_score() {
    let mut game = game();
    assert_eq!(game.public.round, 1);
    assert_eq!(game.public.active_player, 0);

    // Play a free minor improvement first: it is the "improvement in front"
    // that E098 counts, and playing it through the engine keeps the log
    // replayable (no hand-edited board).
    play_minor(&mut game, "B061");
    advance_to_seat0_with_lessons_open(&mut game);
    play_occupation(&mut game, "E098");
    assert_eq!(
        game.stored_bonus_points(0),
        1,
        "first occupation with one improvement in front earns exactly 1 point"
    );
    assert_eq!(
        game.public.players[0].card_points, 0,
        "printed points stay 0"
    );

    // Round 2 is still stage 1, so E101 pays 5 points for the 5 unstarted
    // stages (stages 2..=6).
    advance_to_seat0_with_lessons_open(&mut game);
    assert_eq!(game.public.round, 2);
    play_occupation(&mut game, "E101");
    assert_eq!(game.stored_bonus_points(0), 6);
    game.validate().unwrap();

    let standings = game.standings();
    assert_eq!(
        standings.scores[0].categories["card_bonus"], 6,
        "stored points join the card_bonus category"
    );
    assert_eq!(standings.scores[0].categories["cards"], 0);
    assert_eq!(standings.scores[1].categories["card_bonus"], 0);

    // Save/load replays the events and checks the stamped integrity hash, which
    // is what covers the ledger (state_hash deliberately excludes it).
    let saved = game.save_json().unwrap();
    let loaded = Game::load_json(&saved).unwrap();
    assert_eq!(loaded.bonus_points, game.bonus_points);
    assert_eq!(loaded.standings(), game.standings());
    assert_eq!(loaded.state_hash(), game.state_hash());

    // A hand-edited amount must be rejected.
    let mut tampered: serde_json::Value = serde_json::from_str(&saved).unwrap();
    tampered["bonus_points"]["0"]["E101"] = serde_json::json!(4);
    assert_eq!(
        Game::load_json(&tampered.to_string()).unwrap_err(),
        "saved game bookkeeping integrity mismatch"
    );
}

#[test]
fn e101_blocks_every_later_occupation() {
    let mut game = game();
    play_minor(&mut game, "B061");
    advance_to_seat0_with_lessons_open(&mut game);
    play_occupation(&mut game, "E098");
    advance_to_seat0_with_lessons_open(&mut game);
    play_occupation(&mut game, "E101");
    game.validate().unwrap();

    // Seat 0 still holds occupations, but E101 forbids playing any of them:
    // Lessons disappears from the candidate list even though it is unused.
    advance_to_seat0_with_lessons_open(&mut game);
    assert!(game.public.round >= 3);
    let legal = game.legal_spaces();
    assert!(
        !legal.contains(&Space::Lessons),
        "E101 must remove the occupation action: {legal:?}"
    );
    assert!(!legal.contains(&Space::ExtraLessons));

    // The ban is enforced at the play-validation choke point, so a direct
    // attempt is rejected as well.
    let hand_occupation = game.cards.hands[0]
        .iter()
        .find(|id| {
            game.card_set
                .definition(id)
                .is_some_and(|card| card.kind == CardKind::Occupation)
        })
        .cloned()
        .expect("seat 0 keeps occupations in hand");
    assert!(!can_play(
        &game.card_set,
        game.card_set.definition(&hand_occupation).unwrap(),
        &game.public.players[0],
        0
    ));
    let error = game.apply(Action {
        space: Space::Lessons,
        detail: Detail::PlayOccupation {
            card_id: hand_occupation,
        },
    });
    assert!(error.is_err(), "an occupation after E101 must be rejected");
}

#[test]
fn e098_pays_nothing_when_it_is_not_the_first_occupation() {
    let mut game = game();
    let first = game.cards.hands[0]
        .iter()
        .find(|id| {
            let card = game.card_set.definition(id).expect("dealt card");
            card.kind == CardKind::Occupation
                && id.as_str() != "E098"
                && id.as_str() != "E101"
                && can_play(&game.card_set, card, &game.public.players[0], 0)
        })
        .cloned()
        .expect("another playable occupation");
    play_occupation(&mut game, &first);
    advance_to_seat0_with_lessons_open(&mut game);
    play_occupation(&mut game, "E098");
    assert_eq!(game.stored_bonus_points(0), 0);
    assert!(game.bonus_points.is_empty());
}

/// The card set used above must stay out of the A/B two-player preset.
#[test]
fn band_e_cards_do_not_enter_the_two_player_preset() {
    let preset: CardSet = verified_basic_card_set(2).unwrap();
    for id in ["E098", "E101"] {
        assert!(preset.definition(id).is_none());
        assert!(verified_band_e_cards().iter().any(|card| card.id == id));
    }
}
