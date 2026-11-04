use agricola_re_ai::engine::RecordedAction;
use agricola_re_ai::model::{Config, Crop, Game, RoundCard, Space, Tile};
use agricola_re_ai::policy::{play_to_end, RandomPolicy, SimpleDetails};

fn played_game(enabled: bool) -> Game {
    let mut game = Game::new(Config {
        players: 2,
        seed: 12345,
        additional_action_space_enabled: enabled,
    });
    play_to_end(&mut game, &RandomPolicy, &SimpleDetails).unwrap();
    game
}

#[test]
fn replay_fails_if_a_harvest_decision_is_missing() {
    for enabled in [false, true] {
        let original = played_game(enabled);
        let mut events = original.log.events.clone();
        let event = events
            .iter_mut()
            .find(|event| {
                matches!(
                    serde_json::from_value::<RecordedAction>(event.payload.clone()),
                    Ok(RecordedAction::Work {
                        harvest_plans: Some(_),
                        ..
                    })
                )
            })
            .expect("at least one harvest");
        let payload = event
            .payload
            .get_mut("Work")
            .unwrap()
            .as_object_mut()
            .unwrap();
        assert!(payload.remove("breeding_plans").is_some());
        let error = Game::replay(original.config.clone(), &events).unwrap_err();
        assert!(error.contains("harvest decisions"), "{error}");

        let mut events = original.log.events.clone();
        let event = events
            .iter_mut()
            .find(|event| {
                matches!(
                    serde_json::from_value::<RecordedAction>(event.payload.clone()),
                    Ok(RecordedAction::Work {
                        harvest_plans: Some(_),
                        ..
                    })
                )
            })
            .unwrap();
        event
            .payload
            .get_mut("Work")
            .unwrap()
            .as_object_mut()
            .unwrap()
            .remove("harvest_plans");
        assert!(Game::replay(original.config.clone(), &events).is_err());
    }
}

#[test]
fn replay_rejects_mismatched_event_metadata() {
    let original = played_game(false);
    let mut events = original.log.events.clone();
    events[0].kind = "free".into();
    assert!(Game::replay(original.config.clone(), &events).is_err());
    let mut events = original.log.events.clone();
    events[0].player = None;
    assert!(Game::replay(original.config.clone(), &events).is_err());
}

#[test]
fn saved_state_must_match_logged_actions_and_versions() {
    let original = played_game(true);
    let json = original.save_json().unwrap();
    assert_eq!(
        Game::load_json(&json).unwrap().state_hash(),
        original.state_hash()
    );

    let mut altered = original.clone();
    altered.public.players[0].supply.food += 1;
    let error = Game::load_json(&altered.save_json().unwrap()).unwrap_err();
    assert!(error.contains("logged action"), "{error}");

    let mut altered = original.clone();
    altered.log.log_version = 0;
    assert!(Game::load_json(&altered.save_json().unwrap()).is_err());

    let mut altered = original.clone();
    altered.log.config.additional_action_space_enabled = false;
    assert!(Game::load_json(&altered.save_json().unwrap()).is_err());
}

#[test]
fn malformed_public_state_is_rejected() {
    let mut g = Game::new(Config::default());
    let old = g.public.players[0].farm[14].tile;
    g.public.players[0].farm[14].tile = Tile::Field {
        crop: Some(Crop::Grain),
        remaining: 4,
    };
    assert!(g.validate().unwrap_err().contains("grain stack"));
    g.public.players[0].farm[14].tile = old;

    g.public.used.insert(Space::DayLaborer);
    assert!(g.validate().unwrap_err().contains("worker placements"));
    g.public.used.clear();

    let revealed = g.public.revealed[0];
    g.hidden_round_deck[0] = revealed;
    assert!(g.validate().unwrap_err().contains("round deck"));
    assert_eq!(RoundCard::ALL.len(), 14);
}
