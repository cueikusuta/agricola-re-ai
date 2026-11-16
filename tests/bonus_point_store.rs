//! Foundation tests for the earned-bonus-point store (`Game::bonus_points`).
//!
//! Cards that award points at play time / round start / harvest time cannot be
//! recomputed from the final farm, so the engine keeps them in a bookkeeping
//! ledger instead of `Player.card_points` (printed points only) or a new
//! `Player` field (inside the frozen v5 `state_hash` tuple). These tests pin the
//! compatibility contract of that store:
//!
//! * `state_hash` and the serialized bytes of a game that never stores points
//!   are unchanged (pinned hashes recorded before the store existed),
//! * the A/B two-player preset keeps its 35 cards and `content_hash`,
//! * the store is validated, survives save/load, is covered by
//!   `integrity_hash`, and reaches the public standings.

use agricola_re_ai::cards::{verified_basic_card_set, CardDefinition, CardKind, CardSet};
use agricola_re_ai::engine::{Action, Detail};
use agricola_re_ai::model::{
    add_bonus_points, bonus_points_total, BonusPointLedger, Config, Game, Space,
    MAX_STORED_BONUS_PER_CARD,
};
use agricola_re_ai::policy::{play_to_end, RandomPolicy, SimpleDetails};

/// Fourteen occupations and fourteen minor improvements: the minimum a
/// two-player set must declare (`CardSet::validate`). Synthetic ids only, no
/// printed text.
fn synthetic_set() -> CardSet {
    let card = |id: String, kind: CardKind| CardDefinition {
        use_limit: None,
        id,
        kind,
        cost: vec![],
        points: 0,
        effects: vec![],
    };
    let mut definitions = Vec::new();
    for index in 0..14 {
        definitions.push(card(format!("T-OCC{index:02}"), CardKind::Occupation));
        definitions.push(card(format!("T-MIN{index:02}"), CardKind::MinorImprovement));
    }
    CardSet { definitions }
}

/// A two-player game where the active seat has really played one occupation
/// through the engine (so save/replay is consistent); the returned id is that
/// card. The store cites a card the player actually owns. The seat is the one
/// that played it (the deal decides who starts).
fn game_with_played_card(seed: u64) -> (Game, usize, String) {
    let mut game = Game::with_cards(
        Config {
            players: 2,
            seed,
            additional_action_space_enabled: false,
        },
        synthetic_set(),
    )
    .unwrap();
    let seat = game.public.active_player;
    let card_id = game.cards.hands[seat]
        .iter()
        .find(|id| id.starts_with("T-OCC"))
        .cloned()
        .expect("the active seat holds an occupation");
    game.apply(Action {
        space: Space::Lessons,
        detail: Detail::PlayOccupation {
            card_id: card_id.clone(),
        },
    })
    .unwrap();
    assert!(game.public.players[seat].played_cards.contains(&card_id));
    (game, seat, card_id)
}

#[test]
fn preset_content_hash_and_game_state_hashes_are_unchanged() {
    // Recorded on origin/main (65f2c2b) before the store existed. The store is
    // bookkeeping: it must not touch the frozen v5 state-hash tuple, the dealt
    // A/B preset, or a full-game replay hash.
    //
    // The carded game's FINAL integrity hash moved once with the E099 chapter:
    // `integrity_hash` covers the residual `event_budget`, and the harvest end
    // now opens its own effect chain (resetting the budget, exactly like the
    // field phase does) so a card's "end of each harvest" award cannot be
    // starved by effects that already ran in the field phase. Both initial
    // hashes, both state hashes and the cardless final integrity hash are
    // unchanged, and an empty store still adds nothing to the tuple.
    let preset = verified_basic_card_set(2).unwrap();
    assert_eq!(preset.definitions.len(), 35);
    assert_eq!(
        preset.content_hash(),
        "e62c95c76bfdb96bd3b82a2affd3bbb86f8a30e4e243d46b65475e287291eccd"
    );

    for (players, seed, cards, initial, final_hash, events, initial_integrity, final_integrity) in [
        (
            2usize,
            7u64,
            false,
            "da6b4fe46a34d43549a30f018f3dc290c1ac812d8ce21b3f184067c35fe9d0a0",
            "d113e290f41c697a36065ee2ef8dfedbeaf3903417b8df17cd7b51646dbb470c",
            56usize,
            "5ead3a497d8e3de3bfc2730c02d648177fbc917a16a3f1fa3bbcc94b17d846c9",
            "408c8f8bdd31096a946220751e89818d15b687b83dd979131b39f880f793795b",
        ),
        (
            2,
            22,
            true,
            "55c61261613bc7a60aca3b449d27ea7657c35453bef7e9ce10ff4cf7c2ce6fc3",
            "f9f6677e2a03fdc225afd15b7a2a4e6b2f5f648af7a496ae60a88080088cd4ab",
            64,
            "83f69f8998455d726eb622152ff1782a00e99c39af3b385fc2d8feea78809212",
            "a985ca610df1cb2ba2e170a88ae2c091f7d8f847aa4ce48492807ba73e2f321f",
        ),
    ] {
        let config = Config {
            players,
            seed,
            additional_action_space_enabled: seed % 2 == 0,
        };
        let mut game = if cards {
            Game::with_cards(config, preset.clone()).unwrap()
        } else {
            Game::new(config)
        };
        assert_eq!(
            game.state_hash(),
            initial,
            "initial hash p{players} seed{seed}"
        );
        assert_eq!(
            game.integrity_hash(),
            initial_integrity,
            "initial integrity hash p{players} seed{seed}"
        );
        play_to_end(&mut game, &RandomPolicy, &SimpleDetails).unwrap();
        assert_eq!(
            game.state_hash(),
            final_hash,
            "space-selected final hash p{players} seed{seed} cards{cards}"
        );
        assert_eq!(
            game.integrity_hash(),
            final_integrity,
            "an empty store must hash exactly the pre-store integrity tuple"
        );
        assert_eq!(game.log.events.len(), events);
        assert!(game.bonus_points.is_empty());
    }
}

#[test]
fn legacy_v5_fixture_round_trip_keeps_an_empty_store() {
    for fixture in [
        include_str!("fixtures/v5/cardless_2p_seed7_r3.json"),
        include_str!("fixtures/v5/cards_2p_seed7_r3.json"),
    ] {
        let game = Game::load_json(fixture).unwrap();
        assert!(game.bonus_points.is_empty());
        let saved = game.save_json().unwrap();
        assert!(
            !saved.contains("bonus_points"),
            "an empty store must not appear in the serialized save"
        );
        assert_eq!(
            Game::load_json(&saved).unwrap().state_hash(),
            game.state_hash()
        );
    }
}

#[test]
fn grants_accumulate_per_card_and_player() {
    let mut ledger = BonusPointLedger::new();
    add_bonus_points(&mut ledger, 0, "E098", 2).unwrap();
    add_bonus_points(&mut ledger, 0, "E098", 1).unwrap();
    add_bonus_points(&mut ledger, 0, "E101", 3).unwrap();
    add_bonus_points(&mut ledger, 1, "E100", 1).unwrap();
    // A zero award never creates an entry.
    add_bonus_points(&mut ledger, 1, "E099", 0).unwrap();
    assert_eq!(bonus_points_total(&ledger, 0), 6);
    assert_eq!(bonus_points_total(&ledger, 1), 1);
    assert_eq!(bonus_points_total(&ledger, 2), 0);
    assert!(!ledger[&1].contains_key("E099"));
    assert!(add_bonus_points(&mut ledger, 0, "E098", -1).is_err());
}

#[test]
fn validate_rejects_unknown_player_unplayed_card_and_bad_amounts() {
    let (game, seat, id) = game_with_played_card(11);
    let other_seat = 1 - seat;
    let unplayed = game.cards.hands[seat][0].clone();

    let mut unknown_player = game.clone();
    unknown_player
        .bonus_points
        .entry(7)
        .or_default()
        .insert(id.clone(), 1);
    assert_eq!(
        unknown_player.validate().unwrap_err(),
        "bonus point store names an unknown player"
    );

    let mut not_owned = game.clone();
    not_owned
        .bonus_points
        .entry(seat)
        .or_default()
        .insert(unplayed, 1);
    assert_eq!(
        not_owned.validate().unwrap_err(),
        "stored bonus points for a card that is not played"
    );

    let mut too_much = game.clone();
    too_much
        .bonus_points
        .entry(seat)
        .or_default()
        .insert(id.clone(), MAX_STORED_BONUS_PER_CARD + 1);
    assert_eq!(
        too_much.validate().unwrap_err(),
        "stored bonus points outside the accepted range"
    );

    let mut zero = game.clone();
    zero.bonus_points
        .entry(seat)
        .or_default()
        .insert(id.clone(), 0);
    assert_eq!(
        zero.validate().unwrap_err(),
        "stored bonus points outside the accepted range"
    );

    let mut ok = game;
    ok.grant_bonus_points(seat, &id, 2).unwrap();
    ok.validate().unwrap();
    assert_eq!(ok.stored_bonus_points(seat), 2);
    assert_eq!(ok.stored_bonus_points(other_seat), 0);
}

#[test]
fn state_hash_ignores_the_store_but_integrity_hash_covers_it() {
    let (mut game, seat, id) = game_with_played_card(11);
    let state_before = game.state_hash();
    let integrity_before = game.integrity_hash();
    let saved_before = game.save_json().unwrap();

    game.grant_bonus_points(seat, &id, 3).unwrap();

    assert_eq!(
        game.state_hash(),
        state_before,
        "the ledger is bookkeeping and must stay outside the v5 hash tuple"
    );
    assert_ne!(
        game.integrity_hash(),
        integrity_before,
        "a stored bonus must be covered by the save integrity hash"
    );
    assert_ne!(game.save_json().unwrap(), saved_before);
}

#[test]
fn save_load_round_trip_preserves_the_store_and_rejects_tampering() {
    let (mut game, seat, id) = game_with_played_card(11);
    game.grant_bonus_points(seat, &id, 2).unwrap();
    game.grant_bonus_points(seat, &id, 1).unwrap();

    let saved = game.save_json().unwrap();
    assert!(saved.contains("bonus_points"));
    let loaded = Game::load_json(&saved).unwrap();
    assert_eq!(loaded.bonus_points, game.bonus_points);
    assert_eq!(loaded.stored_bonus_points(seat), 3);
    assert_eq!(loaded.integrity_hash(), game.integrity_hash());

    // Tampering with a stored amount must fail the integrity check even though
    // state_hash cannot see the ledger.
    let mut tampered: serde_json::Value = serde_json::from_str(&saved).unwrap();
    tampered["bonus_points"][seat.to_string()][&id] = serde_json::json!(9);
    let error = Game::load_json(&tampered.to_string()).unwrap_err();
    assert_eq!(error, "saved game bookkeeping integrity mismatch");
}

#[test]
fn standings_count_stored_bonus_points_in_card_bonus() {
    let (game, seat, id) = game_with_played_card(11);
    let other_seat = 1 - seat;
    let before = game.standings();
    let mut with_bonus = game.clone();
    with_bonus.grant_bonus_points(seat, &id, 3).unwrap();
    let after = with_bonus.standings();

    assert_eq!(after.scores[seat].total, before.scores[seat].total + 3);
    assert_eq!(
        after.scores[other_seat].total,
        before.scores[other_seat].total
    );
    assert_eq!(
        after.scores[seat].categories["card_bonus"],
        before.scores[seat].categories["card_bonus"] + 3
    );
    assert_eq!(
        after.scores[seat].categories["cards"], before.scores[seat].categories["cards"],
        "printed card points must stay untouched"
    );
    assert_eq!(after.rank[seat], 1);
}

#[test]
fn observation_mirrors_the_public_store_without_exposing_hands() {
    let (mut game, seat, id) = game_with_played_card(11);
    game.grant_bonus_points(seat, &id, 2).unwrap();

    let obs = game.observation(seat).unwrap();
    assert_eq!(obs.bonus_points, game.bonus_points);
    assert_eq!(
        obs.bonus_points,
        game.observation(1 - seat).unwrap().bonus_points
    );
    // Public mirror only: the opponent's hand stays private.
    assert_eq!(obs.hand, game.cards.hands[seat]);
    assert_eq!(
        obs.hand_counts,
        vec![game.cards.hands[0].len(), game.cards.hands[1].len()]
    );
    assert!(!obs
        .hand
        .iter()
        .any(|card| game.cards.hands[1 - seat].contains(card)));
}
