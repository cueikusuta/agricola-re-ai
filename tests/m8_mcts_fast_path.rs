//! M8-3: the search-only fast path skips state hashing and event logging for
//! hypothetical MCTS positions, but committed games must keep a complete,
//! hash-checked log that survives save/load.
use agricola_re_ai::engine::{Action, Detail};
use agricola_re_ai::model::{Config, Game, Space};

fn new_game() -> Game {
    Game::new(Config {
        players: 2,
        seed: 5,
        additional_action_space_enabled: false,
    })
}

#[test]
fn search_only_games_skip_logging_without_changing_state() {
    let mut commit = new_game();
    let mut search = commit.clone();
    search.record_events = false;
    let action = Action {
        space: Space::DayLaborer,
        detail: Detail::None,
    };
    commit.apply(action.clone()).unwrap();
    search.apply(action).unwrap();
    assert_eq!(commit.log.events.len(), 1);
    assert!(search.log.events.is_empty());
    assert_eq!(
        commit.state_hash(),
        search.state_hash(),
        "skipping the log must not change the game state"
    );
    assert_eq!(commit.public.players[0], search.public.players[0]);
}

#[test]
fn committed_saves_restore_logging_and_omit_the_transient_flag() {
    let mut game = new_game();
    game.apply(Action {
        space: Space::Forest,
        detail: Detail::None,
    })
    .unwrap();
    let json = game.save_json().unwrap();
    assert!(
        !json.contains("record_events"),
        "the transient search flag must never be serialized"
    );
    let loaded = Game::load_json(&json).unwrap();
    assert!(loaded.record_events);
    assert_eq!(loaded.state_hash(), game.state_hash());
    assert_eq!(loaded.log.events.len(), game.log.events.len());
}
