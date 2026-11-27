//! Tests for card accumulation state (resources stored on played cards).
//!
//! This test verifies that the new `card_accumulations` field in `PublicState`
//! functions correctly: save/load round-trip, integrity_hash inclusion,
//! state_hash exclusion (v5 compatibility), and validation.

use agricola_re_ai::cards::verified_basic_card_set;
use agricola_re_ai::cards::CardKind;
use agricola_re_ai::engine::{Action, Detail};
use agricola_re_ai::model::{Config, Game, Good, PublicState, Space};

/// The new `card_accumulations` field must exist in `PublicState` and be
/// serializable with `serde(default)` for backward compatibility.
#[test]
fn card_accumulations_field_exists_and_serializes() {
    // This test will fail to compile until the field is added to PublicState.
    let state = PublicState {
        round: 1,
        active_player: 0,
        start_player: 0,
        used: std::collections::BTreeSet::new(),
        revealed: vec![],
        accumulations: std::collections::BTreeMap::new(),
        animal_accumulations: std::collections::BTreeMap::new(),
        players: vec![],
        improvement_market: vec![],
        work_quota: vec![2, 2],
        workers_placed: vec![0, 0],
        additional_action_space_enabled: false,
        finished: false,
        well_food_schedule: std::collections::BTreeMap::new(),
        card_uses: std::collections::BTreeMap::new(),
        card_uses_game: std::collections::BTreeMap::new(),
        // New field - will cause compile error until added
        card_accumulations: std::collections::BTreeMap::new(),
    };

    // Test serde round-trip
    let json = serde_json::to_string(&state).unwrap();
    let back: PublicState = serde_json::from_str(&json).unwrap();
    assert_eq!(back.card_accumulations, state.card_accumulations);

    // Test legacy JSON (without the field) loads with default empty map
    let legacy_json = r#"{"round":1,"active_player":0,"start_player":0,"used":[],"revealed":[],"accumulations":{},"animal_accumulations":{},"players":[],"improvement_market":[],"work_quota":[2,2],"workers_placed":[0,0],"additional_action_space_enabled":false,"finished":false,"well_food_schedule":{},"card_uses":{}}"#;
    let legacy: PublicState = serde_json::from_str(legacy_json).unwrap();
    assert!(legacy.card_accumulations.is_empty());
}

fn play_occupation_via_lessons(game: &mut Game) -> (usize, String) {
    let active = game.public.active_player;
    let occupation_id = game.cards.hands[active]
        .iter()
        .find(|id| {
            game.card_set
                .definition(id)
                .map(|c| c.kind == CardKind::Occupation)
                .unwrap_or(false)
        })
        .cloned()
        .expect("active player must have an occupation");
    game.apply(Action {
        space: Space::Lessons,
        detail: Detail::PlayOccupation {
            card_id: occupation_id.clone(),
        },
    })
    .unwrap();
    (active, occupation_id)
}

/// Card accumulations survive save/load round-trip with state_hash integrity.
/// Note: replay does not yet preserve card_accumulations because no events
/// modify it (no card effects use it yet). This is expected behavior.
#[test]
fn card_accumulations_survive_save_load_replay() {
    let config = Config {
        players: 2,
        seed: 42,
        additional_action_space_enabled: false,
    };
    let card_set = verified_basic_card_set(2).unwrap();
    let mut game = Game::with_cards(config.clone(), card_set).unwrap();

    // Play a card via Lessons for the active player.
    let (player_idx, occupation_id) = play_occupation_via_lessons(&mut game);

    // Add accumulations to the played card.
    game.public
        .card_accumulations
        .entry(player_idx)
        .or_default()
        .entry(occupation_id.clone())
        .or_default()
        .insert(Good::Wood, 3);
    game.public
        .card_accumulations
        .entry(player_idx)
        .or_default()
        .entry(occupation_id.clone())
        .or_default()
        .insert(Good::Food, 1);

    let original_hash = game.state_hash();
    let original_integrity = game.integrity_hash();
    let original_public = game.public.clone();

    // Save and load
    let json = game.save_json().unwrap();
    let loaded = Game::load_json(&json).unwrap();

    // state_hash must be unchanged (v5 compatibility - card_accumulations NOT in hash tuple)
    assert_eq!(loaded.state_hash(), original_hash);
    assert_eq!(loaded.public, original_public);

    // integrity_hash must match after load (data integrity preserved)
    assert_eq!(loaded.integrity_hash(), original_integrity);

    // The loaded game (before replay) must have the same card_accumulations
    assert_eq!(
        loaded.public.card_accumulations,
        original_public.card_accumulations
    );

    // Replay from loaded game - note: replayed game will have empty card_accumulations
    // because no events modify it yet. This is expected until card effects are added.
    let replayed = match (
        loaded.log.initial_hands.clone(),
        loaded.log.initial_start_player_override,
    ) {
        (Some(hands), Some(starter)) => Game::replay_with_setup_and_start_player(
            loaded.config.clone(),
            (*loaded.card_set).clone(),
            Some(hands),
            Some(starter),
            &loaded.log.events,
        ),
        (Some(hands), None) => Game::replay_with_setup(
            loaded.config.clone(),
            (*loaded.card_set).clone(),
            Some(hands),
            &loaded.log.events,
        ),
        (None, None) => Game::replay_with_cards(
            loaded.config.clone(),
            (*loaded.card_set).clone(),
            &loaded.log.events,
        ),
        (None, Some(_)) => panic!("start player override requires draft hands"),
    }
    .unwrap();

    // state_hash matches (replay reconstructs the game state correctly)
    assert_eq!(replayed.state_hash(), original_hash);
    // replayed game has empty card_accumulations (no events to reconstruct it)
    // This is expected until card effects that modify it are implemented.
    assert!(replayed.public.card_accumulations.is_empty());
}

/// state_hash excludes card_accumulations (v5 pin), integrity_hash includes it.
#[test]
fn state_hash_excludes_card_accumulations_integrity_includes() {
    let config = Config {
        players: 2,
        seed: 123,
        additional_action_space_enabled: false,
    };
    let card_set = verified_basic_card_set(2).unwrap();
    let mut game = Game::with_cards(config.clone(), card_set).unwrap();

    // Play a card via Lessons for the active player.
    let (player_idx, occupation_id) = play_occupation_via_lessons(&mut game);

    let base_state_hash = game.state_hash();
    let base_integrity = game.integrity_hash();

    // Add card accumulations
    game.public
        .card_accumulations
        .entry(player_idx)
        .or_default()
        .entry(occupation_id)
        .or_default()
        .insert(Good::Wood, 5);

    // state_hash must NOT change (v5 compatibility)
    assert_eq!(
        game.state_hash(),
        base_state_hash,
        "card_accumulations must not affect state_hash (v5 pin)"
    );

    // integrity_hash MUST change
    assert_ne!(
        game.integrity_hash(),
        base_integrity,
        "card_accumulations must affect integrity_hash"
    );
}

/// Tampered card_accumulations are detected by integrity_hash on load.
#[test]
fn integrity_hash_detects_tampered_accumulations() {
    let config = Config {
        players: 2,
        seed: 456,
        additional_action_space_enabled: false,
    };
    let card_set = verified_basic_card_set(2).unwrap();
    let mut game = Game::with_cards(config.clone(), card_set).unwrap();

    // Play a card via Lessons for the active player.
    let (player_idx, occupation_id) = play_occupation_via_lessons(&mut game);

    // Add valid accumulation
    game.public
        .card_accumulations
        .entry(player_idx)
        .or_default()
        .entry(occupation_id.clone())
        .or_default()
        .insert(Good::Wood, 1);

    // Save the game
    let json = game.save_json().unwrap();

    // Tamper with the JSON: modify card_accumulations (use lowercase enum as serde expects)
    let mut value: serde_json::Value = serde_json::from_str(&json).unwrap();
    value["public"]["card_accumulations"][player_idx.to_string()][&occupation_id]["wood"] =
        serde_json::json!(999);
    let tampered_json = serde_json::to_string(&value).unwrap();

    // Loading tampered JSON should fail integrity check
    let result = Game::load_json(&tampered_json);
    assert!(
        result.is_err(),
        "tampered accumulations should be rejected: {:?}",
        result
    );
    let err = result.unwrap_err();
    assert!(
        err.contains("integrity") || err.contains("bookkeeping"),
        "error should mention integrity/bookkeeping: {}",
        err
    );
}

/// Validation rejects tampered card_accumulations (e.g., unknown player, unknown card).
#[test]
fn validation_rejects_invalid_card_accumulations() {
    let config = Config {
        players: 2,
        seed: 999,
        additional_action_space_enabled: false,
    };
    let card_set = verified_basic_card_set(2).unwrap();
    let mut game = Game::with_cards(config.clone(), card_set).unwrap();

    // Play a card via Lessons for the active player.
    let (player_idx, occupation_id) = play_occupation_via_lessons(&mut game);

    // Add valid accumulation
    game.public
        .card_accumulations
        .entry(player_idx)
        .or_default()
        .entry(occupation_id.clone())
        .or_default()
        .insert(Good::Wood, 1);

    // Valid state should pass
    game.validate().unwrap();

    // Unknown player index (>= player count)
    game.public
        .card_accumulations
        .entry(2)
        .or_default()
        .entry("CARD".to_string())
        .or_default()
        .insert(Good::Wood, 1);
    assert!(game.validate().is_err());
    game.public.card_accumulations.clear();

    // Unplayed card (card not in player's played_cards)
    game.public
        .card_accumulations
        .entry(player_idx)
        .or_default()
        .entry("UNPLAYED_CARD".to_string())
        .or_default()
        .insert(Good::Wood, 1);
    assert!(game.validate().is_err());
    game.public.card_accumulations.clear();

    // Zero amount (should not exist in resource map)
    game.public
        .card_accumulations
        .entry(player_idx)
        .or_default()
        .entry(occupation_id.clone())
        .or_default()
        .insert(Good::Wood, 0);
    assert!(game.validate().is_err());
    game.public.card_accumulations.clear();

    // Valid again after clearing
    game.validate().unwrap();
}

/// Observation projection includes card_accumulations.
#[test]
fn observation_includes_card_accumulations() {
    let config = Config {
        players: 2,
        seed: 555,
        additional_action_space_enabled: false,
    };
    let card_set = verified_basic_card_set(2).unwrap();
    let mut game = Game::with_cards(config.clone(), card_set).unwrap();

    // Play a card via Lessons for the active player.
    let (player_idx, occupation_id) = play_occupation_via_lessons(&mut game);

    // Add accumulations
    game.public
        .card_accumulations
        .entry(player_idx)
        .or_default()
        .entry(occupation_id)
        .or_default()
        .insert(Good::Reed, 2);

    let obs = game.observation(0).unwrap();
    assert_eq!(
        obs.public.card_accumulations,
        game.public.card_accumulations
    );

    let obs1 = game.observation(1).unwrap();
    assert_eq!(
        obs1.public.card_accumulations,
        game.public.card_accumulations
    );
}

/// Regression test: saves without `card_accumulations` field (old format)
/// must load successfully with the new code. The integrity_hash for empty
/// ledger must be computed identically whether the field is present or not.
#[test]
fn old_save_without_card_accumulations_loads() {
    // Build a game with the new code, save it, then strip card_accumulations
    // from the JSON and verify it loads with the SAME integrity_hash.
    // This verifies the backward-compatible hash computation for empty ledger.
    let config = Config {
        players: 2,
        seed: 42,
        additional_action_space_enabled: false,
    };
    let card_set = verified_basic_card_set(2).unwrap();
    let game = Game::with_cards(config.clone(), card_set.clone()).unwrap();

    // Save with new code (stamps integrity_hash with empty card_accumulations)
    let json = game.save_json().unwrap();
    let mut value: serde_json::Value = serde_json::from_str(&json).unwrap();

    // Capture the integrity_hash that new code produces for empty ledger
    let new_integrity_hash = value["log"]["integrity_hash"].as_str().unwrap().to_string();

    // Remove card_accumulations from public state (simulating old save format)
    value["public"]
        .as_object_mut()
        .unwrap()
        .remove("card_accumulations");

    // The integrity_hash should already match (new code computes same hash for empty)
    let old_save_json = serde_json::to_string(&value).unwrap();

    // This should load without "saved game bookkeeping integrity mismatch"
    let loaded = Game::load_json(&old_save_json).expect("old save must load");

    // Verify card_accumulations is empty (default)
    assert!(loaded.public.card_accumulations.is_empty());

    // Verify integrity_hash matches the stamped value (backward compatible)
    assert_eq!(loaded.integrity_hash(), new_integrity_hash);

    // Verify state_hash matches the initial state hash
    assert_eq!(loaded.state_hash(), loaded.log.initial_state_hash);
}

/// Regression test: saving a game with empty `card_accumulations` must not
/// include the field in JSON (byte-identical to pre-PR saves).
#[test]
fn empty_card_accumulations_save_excludes_field() {
    let config = Config {
        players: 2,
        seed: 999,
        additional_action_space_enabled: false,
    };
    let card_set = verified_basic_card_set(2).unwrap();
    let game = Game::with_cards(config, card_set).unwrap();

    let json = game.save_json().unwrap();
    let value: serde_json::Value = serde_json::from_str(&json).unwrap();

    // card_accumulations must not appear in public state when empty
    assert!(
        !value["public"]
            .as_object()
            .unwrap()
            .contains_key("card_accumulations"),
        "empty card_accumulations must not be serialized: {}",
        value["public"]
    );

    // card_use_limits must not appear when empty (existing PR #327 pattern)
    assert!(
        !value.as_object().unwrap().contains_key("card_use_limits"),
        "empty card_use_limits must not be serialized"
    );

    // well_food_schedule uses #[serde(default)] only, so it appears as {}
    // This is intentional - changing it would break other compatibility.
    assert!(value["public"]["well_food_schedule"].is_object());
}
