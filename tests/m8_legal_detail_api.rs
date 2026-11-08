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

#[test]
fn used_space_is_no_longer_available_and_cannot_be_reused() {
    let mut game = Game::new(Config {
        players: 2,
        seed: 3,
        additional_action_space_enabled: false,
    });
    game.apply(Action {
        space: Space::DayLaborer,
        detail: Detail::None,
    })
    .unwrap();
    assert!(!game.available_spaces().contains(&Space::DayLaborer));
    let before = game.state_hash();
    assert!(game
        .apply(Action {
            space: Space::DayLaborer,
            detail: Detail::None,
        })
        .is_err());
    assert_eq!(game.state_hash(), before);
}

#[test]
fn additional_spaces_stay_mutually_exclusive_through_available_spaces() {
    let mut game = Game::new(Config {
        players: 2,
        seed: 0,
        additional_action_space_enabled: true,
    });
    assert!(game.available_spaces().contains(&Space::ResourceMarket));
    assert!(game.available_spaces().contains(&Space::ModestFamilyGrowth));
    game.apply(Action {
        space: Space::ResourceMarket,
        detail: Detail::None,
    })
    .unwrap();
    assert!(!game.available_spaces().contains(&Space::ModestFamilyGrowth));
    let before = game.state_hash();
    assert!(game
        .apply(Action {
            space: Space::ModestFamilyGrowth,
            detail: Detail::None,
        })
        .is_err());
    assert_eq!(game.state_hash(), before);
}

#[test]
fn modest_family_growth_is_rejected_before_round_five() {
    let mut game = Game::new(Config {
        players: 2,
        seed: 0,
        additional_action_space_enabled: true,
    });
    assert_eq!(game.public.round, 1);
    // Structurally available, and a free housing slot exists, so only the
    // round-5 rule can reject the move.
    assert!(game.available_spaces().contains(&Space::ModestFamilyGrowth));
    let active = game.public.active_player;
    game.public.players[active].extra_housing = 1;
    let action = Action {
        space: Space::ModestFamilyGrowth,
        detail: Detail::None,
    };
    let error = game.validate_action(&action).unwrap_err();
    assert!(error.contains("round 5"), "unexpected error: {error}");
    let before = game.state_hash();
    assert!(game.apply(action).is_err());
    assert_eq!(game.state_hash(), before);
}

#[test]
fn detail_hints_list_legal_elementary_parameters() {
    let game = Game::new(Config {
        players: 2,
        seed: 0,
        additional_action_space_enabled: false,
    });
    let plow = game.detail_hints(Space::Farmland);
    assert!(plow.plow_cells.contains(&1));
    assert!(!plow.plow_cells.contains(&0), "a room cannot be plowed");
    assert_eq!(plow.plow_cells.len(), 13);
    let build = game.detail_hints(Space::FarmExpansion);
    assert!(build.room_cells.contains(&1));
    assert!(build.room_cells.contains(&6));
    assert!(!build.room_cells.contains(&0));
    assert_eq!(build.stable_cells.len(), 13);
}

#[test]
fn detail_hints_never_include_the_opponent_hand() {
    use agricola_re_ai::cards::verified_basic_card_set;
    let game = Game::with_cards(
        Config {
            players: 2,
            seed: 7,
            additional_action_space_enabled: false,
        },
        verified_basic_card_set(2).unwrap(),
    )
    .unwrap();
    let active = game.public.active_player;
    let own = game.observation(active).unwrap().hand;
    let hints = game.detail_hints(Space::Lessons);
    assert!(
        hints.occupations.iter().all(|id| own.contains(id)),
        "occupation hints must come only from the active player's hand"
    );
    let major = game.detail_hints(Space::MajorImprovement);
    assert!(major.minors.iter().all(|id| own.contains(id)));
}

#[test]
fn web_view_exposes_staged_detail_hints_and_available_spaces() {
    use agricola_re_ai::web::Session;
    let mut session = Session::new(Config {
        players: 2,
        seed: 0,
        additional_action_space_enabled: false,
    })
    .unwrap();
    if session.view()["public"]["active_player"] != 0 {
        session.advance_cpu().unwrap();
    }
    let view = session.view();
    assert!(view["available"].as_array().is_some_and(|v| !v.is_empty()));
    assert!(view["detail_hints"].is_object());
    assert!(view["detail_hints"]["farmland"]["plow_cells"].is_array());
}

// --- #109 follow-up: non-default legal details for the other detail spaces ---
//
// The original suite proved the fence case. These tests extend the same
// property to plowing, sowing and animal placement: the engine accepts a legal
// detail that `SimpleDetails` never proposes, and rejects an illegal variant of
// the same shape without mutating state. This is what "the API can express every
// legal detail" means in practice; it is still not an enumeration of the whole
// space.

/// Find a seed whose first revealed card is `card`, so that space is available.
fn game_with_round_card(card: RoundCard) -> Game {
    for seed in 0..2_000u64 {
        let game = Game::new(Config {
            players: 2,
            seed,
            additional_action_space_enabled: false,
        });
        if game.public.revealed.first() == Some(&card) {
            return game;
        }
    }
    panic!("no seed revealed {card:?} in round 1");
}

#[test]
fn plow_detail_can_target_a_cell_the_default_policy_never_picks() {
    use agricola_re_ai::model::Tile;

    let mut game = Game::new(Config {
        players: 2,
        seed: 17,
        additional_action_space_enabled: false,
    });
    assert!(game.available_spaces().contains(&Space::Farmland));
    let player = game.public.active_player;
    let observation = game.observation(player).unwrap();
    let proposed = SimpleDetails
        .produce(&observation, Space::Farmland)
        .expect("default policy proposes a plow target");
    let Detail::Plow { cell: default_cell } = proposed.detail else {
        panic!("the default farmland detail is a plow");
    };
    // Any other empty, stable-free cell is legal too; the default policy only
    // ever names the first one.
    let other = (0..15)
        .find(|&index| {
            index != default_cell
                && matches!(game.public.players[player].farm[index].tile, Tile::Empty)
                && !game.public.players[player].farm[index].stable
        })
        .expect("a second empty cell exists");
    let action = Action {
        space: Space::Farmland,
        detail: Detail::Plow { cell: other },
    };
    let before = game.state_hash();
    game.validate_action(&action)
        .expect("a non-default plow cell is legal");
    assert_eq!(
        game.state_hash(),
        before,
        "validation must not mutate state"
    );
    game.apply(action).expect("engine applies the chosen cell");
    assert!(matches!(
        game.public.players[player].farm[other].tile,
        Tile::Field { crop: None, .. }
    ));
    game.validate().unwrap();
}

#[test]
fn plow_detail_rejects_a_room_cell_atomically() {
    let mut game = Game::new(Config {
        players: 2,
        seed: 17,
        additional_action_space_enabled: false,
    });
    let before = game.state_hash();
    let room = (0..15)
        .find(|&index| {
            !matches!(
                game.public.players[game.public.active_player].farm[index].tile,
                agricola_re_ai::model::Tile::Empty
            )
        })
        .expect("the starting rooms are not empty");
    let action = Action {
        space: Space::Farmland,
        detail: Detail::Plow { cell: room },
    };
    assert!(game.validate_action(&action).is_err());
    assert!(game.apply(action).is_err());
    assert_eq!(game.state_hash(), before);
}

#[test]
fn sow_detail_can_choose_a_layout_the_default_policy_never_picks() {
    use agricola_re_ai::engine::Plant;
    use agricola_re_ai::model::{Crop, Tile};

    let mut game = game_with_round_card(RoundCard::GrainUtilization);
    let player = game.public.active_player;
    // Two empty fields and one grain plus one vegetable: the default policy
    // fills the lowest field with grain, so grain-in-the-second-field is outside
    // its candidate set.
    for cell in [1usize, 2] {
        game.public.players[player].farm[cell].tile = Tile::Field {
            crop: None,
            remaining: 0,
        };
    }
    game.public.players[player].supply.grain = 1;
    game.public.players[player].supply.vegetable = 1;
    let observation = game.observation(player).unwrap();
    let proposed = SimpleDetails
        .produce(&observation, Space::GrainUtilization)
        .expect("default policy proposes a sowing");
    let Detail::SowDetailed {
        plantings: default_plantings,
        ..
    } = &proposed.detail
    else {
        panic!("the default grain-utilization detail is a detailed sowing");
    };
    let chosen = vec![
        Plant {
            cell: 1,
            crop: Crop::Vegetable,
        },
        Plant {
            cell: 2,
            crop: Crop::Grain,
        },
    ];
    assert_ne!(default_plantings, &chosen);
    let action = Action {
        space: Space::GrainUtilization,
        detail: Detail::Sow {
            plantings: chosen.clone(),
            bake: 0,
        },
    };
    game.validate_action(&action)
        .expect("a non-default but legal sowing layout is accepted");
    game.apply(action)
        .expect("engine applies the chosen layout");
    assert!(matches!(
        game.public.players[player].farm[1].tile,
        Tile::Field {
            crop: Some(Crop::Vegetable),
            remaining: 2
        }
    ));
    assert!(matches!(
        game.public.players[player].farm[2].tile,
        Tile::Field {
            crop: Some(Crop::Grain),
            remaining: 3
        }
    ));
    game.validate().unwrap();
}

#[test]
fn sow_detail_rejects_a_duplicate_or_non_field_cell() {
    use agricola_re_ai::engine::Plant;
    use agricola_re_ai::model::{Crop, Tile};

    let mut game = game_with_round_card(RoundCard::GrainUtilization);
    let player = game.public.active_player;
    game.public.players[player].farm[1].tile = Tile::Field {
        crop: None,
        remaining: 0,
    };
    game.public.players[player].supply.grain = 2;
    let before = game.state_hash();
    for plantings in [
        // The same field twice.
        vec![
            Plant {
                cell: 1,
                crop: Crop::Grain,
            },
            Plant {
                cell: 1,
                crop: Crop::Grain,
            },
        ],
        // A room is not a field.
        vec![Plant {
            cell: 0,
            crop: Crop::Grain,
        }],
    ] {
        let action = Action {
            space: Space::GrainUtilization,
            detail: Detail::Sow { plantings, bake: 0 },
        };
        assert!(game.validate_action(&action).is_err());
        assert!(game.apply(action).is_err());
        assert_eq!(game.state_hash(), before);
    }
}

#[test]
fn animal_detail_can_place_stock_where_the_default_policy_never_does() {
    use agricola_re_ai::engine::AnimalPlan;
    use agricola_re_ai::model::{Animal, Tile};

    let mut game = game_with_round_card(RoundCard::SheepMarket);
    let player = game.public.active_player;
    // Two stables and a one-cell pasture. The default policy takes the pet
    // first, then the pasture, so "both sheep in stables" is outside its set.
    let stables = (0..15)
        .filter(|&index| {
            matches!(game.public.players[player].farm[index].tile, Tile::Empty)
                && !game.public.players[player].farm[index].stable
        })
        .take(2)
        .collect::<Vec<_>>();
    assert_eq!(stables.len(), 2, "two empty cells exist");
    for stable in &stables {
        game.public.players[player].farm[*stable].stable = true;
    }
    game.public
        .animal_accumulations
        .insert(Space::SheepMarket, 2);
    game.public.players[player].pet = None;
    let observation = game.observation(player).unwrap();
    let proposed = SimpleDetails
        .produce(&observation, Space::SheepMarket)
        .expect("default policy proposes an animal placement");
    let Detail::Animals(default_plan) = proposed.detail else {
        panic!("the default sheep-market detail places animals");
    };
    let chosen = AnimalPlan {
        animal: Animal::Sheep,
        to_pastures: vec![],
        to_stables: stables.clone(),
        pet: false,
        cook: 0,
        discard: 0,
    };
    assert_ne!(default_plan, chosen);
    let action = Action {
        space: Space::SheepMarket,
        detail: Detail::Animals(chosen),
    };
    game.validate_action(&action)
        .expect("a stable-only placement is legal when capacity allows");
    game.apply(action).expect("engine applies the placement");
    for stable in &stables {
        assert_eq!(
            game.public.players[player].farm[*stable].stable_animal,
            Some(Animal::Sheep)
        );
    }
    game.validate().unwrap();
}

#[test]
fn animal_detail_rejects_a_plan_that_exceeds_capacity() {
    use agricola_re_ai::engine::AnimalPlan;
    use agricola_re_ai::model::{Animal, Tile};

    let mut game = game_with_round_card(RoundCard::SheepMarket);
    let player = game.public.active_player;
    let stable = (0..15)
        .find(|&index| {
            matches!(game.public.players[player].farm[index].tile, Tile::Empty)
                && !game.public.players[player].farm[index].stable
        })
        .expect("an empty cell exists");
    game.public.players[player].farm[stable].stable = true;
    game.public
        .animal_accumulations
        .insert(Space::SheepMarket, 2);
    let before = game.state_hash();
    // Two sheep into one stable, which holds one.
    let action = Action {
        space: Space::SheepMarket,
        detail: Detail::Animals(AnimalPlan {
            animal: Animal::Sheep,
            to_pastures: vec![],
            to_stables: vec![stable, stable],
            pet: false,
            cook: 0,
            discard: 0,
        }),
    };
    assert!(game.validate_action(&action).is_err());
    assert!(game.apply(action).is_err());
    assert_eq!(game.state_hash(), before);
}
