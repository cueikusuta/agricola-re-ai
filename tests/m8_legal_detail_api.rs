//! M8-2: the rule engine must accept every legal detail, not only the ones
//! the default AI candidate generator happens to produce. `SimpleDetails` is a
//! policy, never a definition of the legal action set. Human input and AI
//! selections share the single transactional validator `Game::validate_action`.
use agricola_re_ai::engine::{Action, Detail};
use agricola_re_ai::model::{Config, Game, RoundCard, Space};
use agricola_re_ai::policy::{DetailPolicy, SimpleDetails};

/// Round 1 reveals one stage-1 card; find a seed that reveals Fencing so the
/// space is available for a direct legal-detail test.
fn game_with_fencing() -> Game {
    for seed in 0..2_000u64 {
        let game = Game::new(Config {
            players: 2,
            seed,
            additional_action_space_enabled: false,
        });
        if game.public.revealed.first() == Some(&RoundCard::Fencing) {
            return game;
        }
    }
    panic!("no seed revealed Fencing in round 1");
}

/// A fresh two-cell pasture (cells 1 and 2) is legal and costs six new fence
/// segments. `SimpleDetails` would instead build a single-cell pasture, so this
/// detail is deliberately outside its candidate set.
fn two_cell_pasture_action() -> (Game, Action) {
    let mut game = game_with_fencing();
    assert!(game.available_spaces().contains(&Space::Fencing));
    let player = game.public.active_player;
    game.public.players[player].supply.wood = 10;
    let action = Action {
        space: Space::Fencing,
        detail: Detail::Fence {
            pastures: vec![vec![1, 2]],
        },
    };
    (game, action)
}

#[test]
fn engine_accepts_a_legal_detail_that_simple_details_never_proposes() {
    let (mut game, action) = two_cell_pasture_action();
    let player = game.public.active_player;
    let observation = game.observation(player).unwrap();
    let proposed = SimpleDetails
        .produce(&observation, Space::Fencing)
        .expect("default policy proposes some fencing detail");
    assert_ne!(
        proposed.detail, action.detail,
        "test detail must lie outside the default candidate generator"
    );

    let before = game.state_hash();
    game.validate_action(&action)
        .expect("shared validator accepts a legal non-default detail");
    assert_eq!(
        game.state_hash(),
        before,
        "validation must not mutate state"
    );
    game.apply(action).expect("engine applies the legal detail");
    let pasture = game.public.players[player]
        .pastures
        .iter()
        .find(|pasture| pasture.cells == vec![1, 2])
        .expect("two-cell pasture exists");
    assert_eq!(pasture.quantity, 0);
    assert_eq!(game.public.players[player].supply.wood, 4);
    game.validate().unwrap();
}

#[test]
fn illegal_details_are_rejected_without_changing_state() {
    let (mut game, _) = two_cell_pasture_action();
    let before = game.state_hash();
    let bad = Action {
        space: Space::Fencing,
        detail: Detail::Fence {
            // Cell 0 is a starting room and cannot be fenced.
            pastures: vec![vec![0, 1]],
        },
    };
    assert!(game.validate_action(&bad).is_err());
    assert!(game.apply(bad).is_err());
    assert_eq!(game.state_hash(), before);
}

#[test]
fn missing_detail_and_unrevealed_space_are_rejected() {
    let (mut game, _) = two_cell_pasture_action();
    let before = game.state_hash();
    let no_detail = Action {
        space: Space::Fencing,
        detail: Detail::None,
    };
    assert!(game.validate_action(&no_detail).is_err());
    assert!(game.apply(no_detail).is_err());

    // MajorImprovement is a hidden round card in this round-1 state.
    assert!(!game.available_spaces().contains(&Space::MajorImprovement));
    let unrevealed = Action {
        space: Space::MajorImprovement,
        detail: Detail::None,
    };
    assert!(game.validate_action(&unrevealed).is_err());
    assert!(game.apply(unrevealed).is_err());
    assert_eq!(game.state_hash(), before);
}

#[test]
fn human_and_ai_paths_agree_on_the_shared_validator() {
    let (game, legal) = two_cell_pasture_action();
    let player = game.public.active_player;
    let observation = game.observation(player).unwrap();
    let ai_detail = SimpleDetails.produce(&observation, Space::Fencing).unwrap();
    // The AI-proposed detail and the human-supplied detail both pass the same
    // validator; a deliberately illegal variant fails it.
    assert!(game.validate_action(&legal).is_ok());
    assert!(game.validate_action(&ai_detail).is_ok());
    let illegal = Action {
        space: Space::Fencing,
        detail: Detail::Fence {
            pastures: vec![vec![3, 4, 5]],
        },
    };
    assert!(game.validate_action(&illegal).is_err());
}
