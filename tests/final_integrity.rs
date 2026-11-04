use agricola_re_ai::engine::{Action, Detail};
use agricola_re_ai::model::{Config, Game, Improvement, Space, Tile};
use agricola_re_ai::policy::{play_to_end, RandomPolicy, SimpleDetails};

fn game() -> Game {
    Game::new(Config { seed: 993_714, additional_action_space_enabled: true })
}

#[test]
fn every_major_improvement_exists_exactly_once() {
    let mut game = game();
    assert!(game.validate().is_ok());
    game.public.players[0].improvements.push(Improvement::Well);
    assert!(game.validate().is_err(), "a copied improvement must be rejected");
    game.public.improvement_market.retain(|card| *card != Improvement::Well);
    assert!(game.validate().is_ok(), "a legally transferred card is valid");
    game.public.improvement_market.push(Improvement::Well);
    assert!(game.validate().is_err(), "market cannot regain an owned card");
}

#[test]
fn disconnected_rooms_and_fields_are_invalid() {
    let mut game = game();
    game.public.players[0].farm[3].tile = Tile::Field { crop: None, remaining: 0 };
    game.public.players[0].farm[14].tile = Tile::Field { crop: None, remaining: 0 };
    assert!(game.validate().is_err());
    game.public.players[0].farm[14].tile = Tile::Empty;
    assert!(game.validate().is_ok());
    game.public.players[0].farm[5].tile = Tile::Empty;
    game.public.players[0].farm[14].tile = Tile::Room;
    assert!(game.validate().is_err());
}

#[test]
fn saving_uses_a_verifiable_initial_state_snapshot() {
    let game = game();
    assert_eq!(game.log.initial_public_state.as_ref(), Some(&game.public));
    assert_eq!(
        Game::load_json(&game.save_json().unwrap()).unwrap().state_hash(),
        game.state_hash()
    );
    let mut value: serde_json::Value = serde_json::from_str(&game.save_json().unwrap()).unwrap();
    value["log"]["initial_public_state"]["start_player"] = serde_json::json!(42);
    assert!(Game::load_json(&serde_json::to_string(&value).unwrap()).is_err());
}

#[test]
fn a_modified_early_replay_event_is_rejected_on_load() {
    let mut game = game();
    game.apply(Action { space: Space::DayLaborer, detail: Detail::None }).unwrap();
    game.apply(Action { space: Space::GrainSeeds, detail: Detail::None }).unwrap();
    let mut value: serde_json::Value = serde_json::from_str(&game.save_json().unwrap()).unwrap();
    // Intentionally leave the hashes untouched: only a full replay can detect this.
    value["log"]["events"][0]["payload"]["Work"]["action"]["space"] =
        serde_json::json!("forest");
    assert!(Game::load_json(&serde_json::to_string(&value).unwrap()).is_err());
}

#[test]
fn full_games_save_and_replay_with_the_initial_snapshot() {
    for additional_action_space_enabled in [false, true] {
        let config = Config { seed: 781, additional_action_space_enabled };
        let mut game = Game::new(config.clone());
        play_to_end(&mut game, &RandomPolicy, &SimpleDetails).unwrap();
        assert!(game.public.finished);
        assert_eq!(Game::replay(config, &game.log.events).unwrap().state_hash(), game.state_hash());
        assert_eq!(Game::load_json(&game.save_json().unwrap()).unwrap().state_hash(), game.state_hash());
    }
}
