//! Reproduction test for well ledger mirror tampering.
//! This test demonstrates that the current load_json verification
//! does not catch divergence between delayed_well and well_food_schedule
//! for v5 saves (without integrity_hash).
//!
//! NOTE: After the fix, `delayed_well` is removed and `well_food_schedule`
//! is the single source of truth. This test verifies that the load_json
//! verification catches tampering with `well_food_schedule` and `card_uses`.

use agricola_re_ai::cards::verified_basic_card_set;
use agricola_re_ai::model::{Config, Game};
use agricola_re_ai::policy::{DetailPolicy, RandomPolicy, SimpleDetails, SpacePolicy};
use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;
use serde_json::Value;

#[test]
fn card_uses_tampering_v5_save_not_detected_by_replay() {
    // Create a game and play a few actions
    let mut game = Game::with_cards(
        Config {
            players: 2,
            seed: 77,
            additional_action_space_enabled: false,
        },
        verified_basic_card_set(2).unwrap(),
    )
    .unwrap();

    // Play a few actions
    let mut rng = ChaCha8Rng::seed_from_u64(0xDEAD_BEEF);
    let policy = RandomPolicy;
    let details = SimpleDetails;

    for _ in 0..4 {
        let id = game.public.active_player;
        let obs = game.observation(id).unwrap();
        let legal = game.legal_spaces();
        if let Some(space) = policy.select(&obs, &legal, &mut rng) {
            if let Some(action) = details.produce(&obs, space) {
                let _ = game.apply(action);
            }
        }
    }

    // Add some card uses (simulating a card was played)
    game.public.card_uses.insert("TEST_CARD".to_string(), 2);

    println!("Original game card_uses: {:?}", game.public.card_uses);
    println!("Events in log: {}", game.log.events.len());

    // Save the game
    let json_str = game.save_json().unwrap();
    let mut value: Value = serde_json::from_str(&json_str).unwrap();

    // Tamper: modify card_uses
    value["public"]["card_uses"] = serde_json::json!({
        "TEST_CARD": 5  // Different count!
    });
    // Remove integrity_hash to simulate pre-Step3 save
    value["log"]
        .as_object_mut()
        .unwrap()
        .remove("integrity_hash");

    let tampered_json = serde_json::to_string(&value).unwrap();

    // Try to load
    let result = Game::load_json(&tampered_json);

    match result {
        Ok(loaded) => {
            println!("LOAD PASSED (BUG: card_uses tampering not detected for v5 save)");
            println!("Loaded card_uses: {:?}", loaded.public.card_uses);
            println!("Loaded state_hash: {}", loaded.state_hash());
            assert!(
                false,
                "card_uses tampering should be detected but load passed"
            );
        }
        Err(e) => {
            println!("LOAD FAILED (correct): {}", e);
            assert!(
                e.contains("card_uses") || e.contains("bookkeeping") || e.contains("integrity"),
                "Should fail due to card_uses mismatch, got: {}",
                e
            );
        }
    }
}

#[test]
fn well_food_schedule_tampering_v5_save_not_detected_by_replay() {
    // Create a game with a well reservation (using the single source of truth)
    let mut game = Game::with_cards(
        Config {
            players: 2,
            seed: 77,
            additional_action_space_enabled: false,
        },
        verified_basic_card_set(2).unwrap(),
    )
    .unwrap();

    // Install a reservation for round 3 (as a real Well purchase would)
    game.public.well_food_schedule.entry(3).or_default().push(0);

    println!(
        "Original game well_food_schedule: {:?}",
        game.public.well_food_schedule
    );
    println!("Events in log: {}", game.log.events.len());

    // Save the game
    let json_str = game.save_json().unwrap();
    let mut value: Value = serde_json::from_str(&json_str).unwrap();

    // Tamper: modify well_food_schedule
    value["public"]["well_food_schedule"] = serde_json::json!({
        "3": [1]  // Different player!
    });
    // Remove integrity_hash to simulate pre-Step3 save
    value["log"]
        .as_object_mut()
        .unwrap()
        .remove("integrity_hash");

    let tampered_json = serde_json::to_string(&value).unwrap();

    // Try to load
    let result = Game::load_json(&tampered_json);

    match result {
        Ok(loaded) => {
            println!("LOAD PASSED (BUG: well_food_schedule tampering not detected for v5 save)");
            println!(
                "Loaded well_food_schedule: {:?}",
                loaded.public.well_food_schedule
            );
            println!("Loaded state_hash: {}", loaded.state_hash());
            assert!(
                false,
                "well_food_schedule tampering should be detected but load passed"
            );
        }
        Err(e) => {
            println!("LOAD FAILED (correct): {}", e);
            // The tampering is now detected either via state_hash (replay verification)
            // or via the explicit well_food_schedule check in load_json.
            assert!(
                e.contains("well_food_schedule")
                    || e.contains("bookkeeping")
                    || e.contains("integrity")
                    || e.contains("logged action replay"),
                "Should fail due to well_food_schedule mismatch, got: {}",
                e
            );
        }
    }
}
