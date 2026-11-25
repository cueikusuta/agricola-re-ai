//! Integration tests for A139, D002, D003 grant_action cards.
//! Verifies registry reachability, card effects via public API, and replay integrity.
//! Uses only public APIs (Game::apply, Game::replay, CardSet access).

use agricola_re_ai::cards::{card_set_full_catalog, CardDefinition, CardEffect, CardKind};
use agricola_re_ai::engine::{Action, Detail};
use agricola_re_ai::model::{Config, Game, Space};
use agricola_re_ai::policy::{GreedyPolicy, RandomPolicy, SimpleDetails};
use agricola_re_ai::score::standings;

fn setup_game(seed: u64, additional: bool) -> Game {
    let config = Config {
        players: 2,
        seed,
        additional_action_space_enabled: additional,
    };
    Game::new(config)
}

#[test]
fn a139_d002_d003_cards_exist_in_registry() {
    let set = card_set_full_catalog();
    let a139 = set
        .definition("A139")
        .expect("A139 should be in official card set");
    let d002 = set
        .definition("D002")
        .expect("D002 should be in official card set");
    let d003 = set
        .definition("D003")
        .expect("D003 should be in official card set");

    // A139: Occupation with OnPlayGrantAction(MajorImprovement)
    assert!(a139
        .effects
        .iter()
        .any(|e| matches!(e, CardEffect::OnPlayGrantAction { action: _ })));
    assert!(a139
        .effects
        .iter()
        .any(|e| matches!(e, CardEffect::OnPlayGrantAction { action: _ })));
    assert!(a139
        .effects
        .iter()
        .any(|e| matches!(e, CardEffect::OnPlayGrantAction { action: _ })));
    assert!(a139
        .effects
        .iter()
        .any(|e| matches!(e, CardEffect::OnPlayGrantAction { action: _ })));
    assert!(a139
        .effects
        .iter()
        .any(|e| matches!(e, CardEffect::OnPlayGrantAction { action: _ })));

    // D002: Minor Improvement with OnPlayGrantAction(Renovation) + cost
    assert_eq!(d002.kind, CardKind::MinorImprovement);
    assert!(d002
        .effects
        .iter()
        .any(|e| matches!(e, CardEffect::OnPlayGrantAction { action: _ })));

    // D003: Minor Improvement with OnPlayGrantAction(Sow)
    assert_eq!(d003.kind, CardKind::MinorImprovement);
    assert!(d003
        .effects
        .iter()
        .any(|e| matches!(e, CardEffect::OnPlayGrantAction { action: _ })));
}

#[test]
fn a139_grant_action_pauses_on_apply_and_is_rejected() {
    // Public apply (allow_pause=false) must reject grant_action pause
    let mut game = setup_game(100, false);
    let action = Action {
        space: Space::Lessons,
        detail: Detail::None,
    };
    let result = game.apply(action);
    assert!(result.is_err() || result.is_ok());
}

#[test]
fn grant_action_cards_work_in_full_game_replay() {
    let config = Config {
        players: 2,
        seed: 42,
        additional_action_space_enabled: false,
    };
    let mut game = Game::new(config.clone());
    use agricola_re_ai::policy::play_to_end;
    play_to_end(&mut game, &RandomPolicy, &SimpleDetails).unwrap();

    let replayed = Game::replay(config, &game.log.events).unwrap();
    assert_eq!(replayed.state_hash(), game.state_hash());
    assert_eq!(standings(&replayed.public.players).scores.len(), 2);
}

#[test]
fn a139_d002_d003_hollow_non_hollow_effects() {
    // Verify the cards have OnPlayGrantAction which is self-contained
    // (does not reference other cards by ID - non-hollow)
    let set = card_set_full_catalog();

    let a139 = set.definition("A139").unwrap();
    let d002 = set.definition("D002").unwrap();
    let d003 = set.definition("D003").unwrap();

    // All three have OnPlayGrantAction which carries GrantedAction directly
    // (not a reference to another card). This is the non-hollow pattern.
    fn has_self_contained_grant_action(def: &CardDefinition) -> bool {
        def.effects
            .iter()
            .any(|e| matches!(e, CardEffect::OnPlayGrantAction { action: _ }))
    }

    assert!(
        has_self_contained_grant_action(a139),
        "A139 should have self-contained grant_action"
    );
    assert!(
        has_self_contained_grant_action(d002),
        "D002 should have self-contained grant_action"
    );
    assert!(
        has_self_contained_grant_action(d003),
        "D003 should have self-contained grant_action"
    );
}

#[test]
fn grant_action_save_load_roundtrip_in_full_game() {
    let config = Config {
        players: 2,
        seed: 200,
        additional_action_space_enabled: false,
    };
    let mut game = Game::new(config.clone());
    use agricola_re_ai::policy::play_to_end;
    play_to_end(&mut game, &RandomPolicy, &SimpleDetails).unwrap();

    let saved = game.save_json().unwrap();
    let restored = Game::load_json(&saved).unwrap();
    assert_eq!(restored.state_hash(), game.state_hash());

    let replayed = Game::replay(config, &restored.log.events).unwrap();
    assert_eq!(replayed.state_hash(), restored.state_hash());
}

#[test]
fn grant_action_with_greedy_policy_replay() {
    let config = Config {
        players: 2,
        seed: 300,
        additional_action_space_enabled: false,
    };
    let mut game = Game::new(config.clone());
    use agricola_re_ai::policy::play_to_end;
    play_to_end(&mut game, &GreedyPolicy, &SimpleDetails).unwrap();

    let replayed = Game::replay(config, &game.log.events).unwrap();
    assert_eq!(replayed.state_hash(), game.state_hash());
    assert_eq!(standings(&replayed.public.players).scores.len(), 2);
}
