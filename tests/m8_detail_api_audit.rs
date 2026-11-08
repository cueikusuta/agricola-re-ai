//! M8-2 (#109): audit of the staged legal-detail API.
//!
//! Every case below is a detail a human may legitimately enter through the UI.
//! The tests separate two questions that must not be conflated
//! (`docs/roadmap-m7-onward.md` §4.2):
//!
//! 1. Does the single transactional validator (`Game::validate_action` /
//!    `Game::apply`) accept the legal detail even when `SimpleDetails` never
//!    proposes it?
//! 2. Does an illegal detail leave the state, hash and log untouched?
//!
//! `SimpleDetails` is an AI candidate generator, never the definition of the
//! legal action set. Details outside its candidate set assert that the proposed
//! detail really differs, so the test cannot silently pass by testing the
//! default policy.
use agricola_re_ai::engine::{
    Action, AnimalLocation, AnimalPlan, BakeAllocation, Detail, FreeAction, Plant, PurchaseMode,
};
use agricola_re_ai::harvest::{FeedSource, FeedingPlan, HarvestDetailPolicy};
use agricola_re_ai::model::{
    Animal, Config, Crop, Game, Good, House, Improvement, Observation, RoundCard, Space, Tile,
};
use agricola_re_ai::policy::{DetailPolicy, SimpleDetails};

fn new_game(players: usize, seed: u64, additional: bool) -> Game {
    Game::new(Config {
        players,
        seed,
        additional_action_space_enabled: additional,
    })
}

/// The seat that is about to act. Seat 0 is not guaranteed: the starting
/// player is drawn from the seed.
fn actor(game: &Game) -> usize {
    game.public.active_player
}

/// Reveal a round card in the current round without touching the rest of the
/// deck, so `available_spaces()` agrees with `validate()`. This is test setup
/// only; the save/replay test below never mutates the state it replays.
fn reveal(game: &mut Game, target: RoundCard) {
    if game.public.revealed.contains(&target) {
        return;
    }
    let index = game
        .hidden_round_deck
        .iter()
        .position(|&card| card == target)
        .expect("card is still hidden");
    let swap = game.public.revealed.len() - 1;
    let displaced = game.public.revealed[swap];
    game.public.revealed[swap] = target;
    game.hidden_round_deck[index] = displaced;
}

/// Move a major improvement from the market to a player, keeping the
/// "exactly one physical copy" invariant of `Game::validate`.
fn give_improvement(game: &mut Game, player: usize, item: Improvement) {
    let index = game
        .public
        .improvement_market
        .iter()
        .position(|&entry| entry == item)
        .expect("improvement starts in the market");
    game.public.improvement_market.remove(index);
    game.public.players[player].improvements.push(item);
}

fn proposal(game: &Game, space: Space) -> Option<Action> {
    let observation = game.observation(actor(game)).unwrap();
    SimpleDetails.produce(&observation, space)
}

fn empty_field() -> Tile {
    Tile::Field {
        crop: None,
        remaining: 0,
    }
}

/* ------------------------------------------------------------------ plow */

#[test]
fn plow_accepts_any_legal_first_field_and_rejects_a_detached_second_field() {
    let game = new_game(2, 5, false);
    let first = proposal(&game, Space::Farmland).unwrap();
    let owner = actor(&game);
    // A first field may go on any empty farm cell; the default policy picks the
    // first one, so a far cell proves the validator is independent of it.
    assert_eq!(first.detail, Detail::Plow { cell: 1 });
    let mut chosen = game.clone();
    let before = chosen.state_hash();
    let action = Action {
        space: Space::Farmland,
        detail: Detail::Plow { cell: 14 },
    };
    chosen.validate_action(&action).unwrap();
    assert_eq!(chosen.state_hash(), before, "validation stays read-only");
    chosen.apply(action).unwrap();
    assert_eq!(chosen.public.players[owner].farm[14].tile, empty_field());
    chosen.validate().unwrap();

    // A second field must touch the existing one. Cell 9 is adjacent to 14,
    // cell 2 is not. The seat that is now active already owns cell 14.
    reveal(&mut chosen, RoundCard::Cultivation);
    let mut probe = chosen.clone();
    let other = actor(&probe);
    assert_ne!(
        other, owner,
        "the placement passes the turn to the opponent"
    );
    probe.public.players[other].farm[14].tile = empty_field();
    let detached = Action {
        space: Space::Cultivation,
        detail: Detail::Cultivate {
            plow: Some(2),
            plantings: vec![],
        },
    };
    let before = probe.state_hash();
    assert!(probe.validate_action(&detached).is_err());
    assert!(probe.apply(detached).is_err());
    assert_eq!(probe.state_hash(), before, "rejection is atomic");
    let touching = Action {
        space: Space::Cultivation,
        detail: Detail::Cultivate {
            plow: Some(9),
            plantings: vec![],
        },
    };
    probe.apply(touching).unwrap();
    assert!(matches!(
        probe.public.players[other].farm[9].tile,
        Tile::Field { .. }
    ));
    probe.validate().unwrap();
}

/* ------------------------------------------------------------------- sow */

#[test]
fn sow_accepts_partial_and_crop_mixed_allocations_the_default_policy_skips() {
    let mut game = new_game(2, 11, false);
    let owner = actor(&game);
    reveal(&mut game, RoundCard::GrainUtilization);
    for cell in 1..=3 {
        game.public.players[owner].farm[cell].tile = empty_field();
    }
    game.public.players[owner].supply.grain = 1;
    game.public.players[owner].supply.vegetable = 1;
    let proposed = proposal(&game, Space::GrainUtilization).unwrap();
    let Detail::SowDetailed { plantings, .. } = &proposed.detail else {
        panic!("expected a sow detail, got {:?}", proposed.detail);
    };
    assert_eq!(plantings.len(), 2, "default policy sows every empty field");
    // Vegetable into the first field, grain into the second, third left empty.
    let human = Detail::Sow {
        plantings: vec![
            Plant {
                cell: 1,
                crop: Crop::Vegetable,
            },
            Plant {
                cell: 2,
                crop: Crop::Grain,
            },
        ],
        bake: 0,
    };
    assert_ne!(proposed.detail, human);
    game.apply(Action {
        space: Space::GrainUtilization,
        detail: human,
    })
    .unwrap();
    let player = &game.public.players[owner];
    assert_eq!(
        player.farm[1].tile,
        Tile::Field {
            crop: Some(Crop::Vegetable),
            remaining: 2
        }
    );
    assert_eq!(
        player.farm[2].tile,
        Tile::Field {
            crop: Some(Crop::Grain),
            remaining: 3
        }
    );
    assert_eq!(
        player.farm[3].tile,
        empty_field(),
        "a player may leave a field unsown"
    );
    assert_eq!(player.supply.grain, 0);
    assert_eq!(player.supply.vegetable, 0);
    game.validate().unwrap();
}

#[test]
fn sow_rejects_missing_seed_and_duplicate_targets_atomically() {
    let mut game = new_game(2, 12, false);
    let owner = actor(&game);
    reveal(&mut game, RoundCard::GrainUtilization);
    for cell in 1..=2 {
        game.public.players[owner].farm[cell].tile = empty_field();
    }
    game.public.players[owner].supply.grain = 1;
    let before = game.state_hash();
    let duplicate = Action {
        space: Space::GrainUtilization,
        detail: Detail::Sow {
            plantings: vec![
                Plant {
                    cell: 1,
                    crop: Crop::Grain,
                },
                Plant {
                    cell: 1,
                    crop: Crop::Grain,
                },
            ],
            bake: 0,
        },
    };
    assert!(game.apply(duplicate).is_err());
    let no_seed = Action {
        space: Space::GrainUtilization,
        detail: Detail::Sow {
            plantings: vec![
                Plant {
                    cell: 1,
                    crop: Crop::Grain,
                },
                Plant {
                    cell: 2,
                    crop: Crop::Grain,
                },
            ],
            bake: 0,
        },
    };
    assert!(game.apply(no_seed).is_err());
    assert_eq!(game.state_hash(), before, "failed sow changes nothing");
    game.validate().unwrap();
}

/* --------------------------------------------------------------- fencing */

#[test]
fn fencing_accepts_two_separate_new_pastures_in_one_action() {
    let mut game = new_game(2, 21, false);
    let owner = actor(&game);
    reveal(&mut game, RoundCard::Fencing);
    game.public.players[owner].supply.wood = 15;
    let proposed = proposal(&game, Space::Fencing).unwrap();
    let Detail::Fence { pastures } = &proposed.detail else {
        panic!("expected a fence detail, got {:?}", proposed.detail);
    };
    assert_eq!(pastures.len(), 1, "default policy builds one pasture");
    let human = Detail::Fence {
        pastures: vec![vec![1, 2], vec![7, 8]],
    };
    assert_ne!(proposed.detail, human);
    game.apply(Action {
        space: Space::Fencing,
        detail: human,
    })
    .unwrap();
    let player = &game.public.players[owner];
    assert_eq!(player.pastures.len(), 2);
    assert_eq!(
        player.fences.len(),
        11,
        "the shared segment is counted once"
    );
    assert_eq!(player.supply.wood, 4);
    game.validate().unwrap();
}

#[test]
fn fencing_rejects_overlapping_or_unaffordable_regions_atomically() {
    let mut game = new_game(2, 22, false);
    let owner = actor(&game);
    reveal(&mut game, RoundCard::Fencing);
    game.public.players[owner].supply.wood = 4;
    let before = game.state_hash();
    let overlapping = Action {
        space: Space::Fencing,
        detail: Detail::Fence {
            pastures: vec![vec![1, 2], vec![2, 3]],
        },
    };
    assert!(game.apply(overlapping).is_err());
    let unaffordable = Action {
        space: Space::Fencing,
        detail: Detail::Fence {
            pastures: vec![vec![1, 2]],
        },
    };
    assert!(
        game.apply(unaffordable).is_err(),
        "six new segments cost more than four wood"
    );
    assert_eq!(game.state_hash(), before);
    assert!(game.public.players[owner].fences.is_empty());
    game.validate().unwrap();
}

/* ----------------------------------------------------------------- build */

#[test]
fn build_accepts_a_room_and_a_stable_in_one_action_on_chosen_cells() {
    let mut game = new_game(2, 31, false);
    let owner = actor(&game);
    game.public.players[owner].supply.wood = 10;
    game.public.players[owner].supply.reed = 2;
    let proposed = proposal(&game, Space::FarmExpansion).unwrap();
    assert_eq!(
        proposed.detail,
        Detail::Build {
            rooms: vec![1],
            stables: vec![],
        },
        "the default policy spends wood on a room first"
    );
    game.apply(Action {
        space: Space::FarmExpansion,
        detail: Detail::Build {
            rooms: vec![6],
            stables: vec![12],
        },
    })
    .unwrap();
    let player = &game.public.players[owner];
    assert_eq!(player.farm[6].tile, Tile::Room);
    assert!(player.farm[12].stable);
    assert_eq!(player.supply.wood, 3, "5 wood room + 2 wood stable");
    assert_eq!(player.supply.reed, 0);
    assert_eq!(player.house, House::Wood);
    game.validate().unwrap();
}

#[test]
fn build_rejects_a_stable_on_a_room_cell_without_charging_for_the_room() {
    let mut game = new_game(2, 32, false);
    let owner = actor(&game);
    game.public.players[owner].supply.wood = 10;
    game.public.players[owner].supply.reed = 2;
    let before = game.state_hash();
    let action = Action {
        space: Space::FarmExpansion,
        detail: Detail::Build {
            rooms: vec![1],
            stables: vec![0],
        },
    };
    assert!(game.validate_action(&action).is_err());
    assert!(game.apply(action).is_err());
    assert_eq!(game.state_hash(), before);
    assert_eq!(game.public.players[owner].supply.wood, 10);
    assert_eq!(game.public.players[owner].supply.reed, 2);
    game.validate().unwrap();
}

/* --------------------------------------------------------------- animals */

/// Real worker placements so the same player owns a pasture and an unfenced
/// stable before the animal market is used.
fn farm_with_pasture_and_stable() -> (Game, usize) {
    let mut game = new_game(2, 41, false);
    let owner = actor(&game);
    reveal(&mut game, RoundCard::Fencing);
    game.public.players[owner].supply.wood = 15;
    game.apply(Action {
        space: Space::Fencing,
        detail: Detail::Fence {
            pastures: vec![vec![13]],
        },
    })
    .unwrap();
    game.apply(Action {
        space: Space::DayLaborer,
        detail: Detail::None,
    })
    .unwrap();
    game.apply(Action {
        space: Space::FarmExpansion,
        detail: Detail::Build {
            rooms: vec![],
            stables: vec![12],
        },
    })
    .unwrap();
    game.apply(Action {
        space: Space::Fishing,
        detail: Detail::None,
    })
    .unwrap();
    assert_eq!(game.public.players[owner].pastures.len(), 1);
    assert!(game.public.players[owner].farm[12].stable);
    (game, owner)
}

#[test]
fn animal_plan_accepts_pasture_stable_pet_cooking_and_discard_together() {
    let (mut game, owner) = farm_with_pasture_and_stable();
    reveal(&mut game, RoundCard::SheepMarket);
    game.public.players[owner].supply.food = 0;
    game.public
        .animal_accumulations
        .insert(Space::SheepMarket, 5);
    let plan = AnimalPlan {
        animal: Animal::Sheep,
        to_pastures: vec![(0, 1)],
        to_stables: vec![12],
        pet: true,
        cook: 1,
        discard: 1,
    };
    // Without a cooking improvement the market cannot be cooked.
    let before = game.state_hash();
    assert!(game
        .apply(Action {
            space: Space::SheepMarket,
            detail: Detail::Animals(plan.clone()),
        })
        .is_err());
    assert_eq!(game.state_hash(), before, "rejected plan keeps the state");
    give_improvement(&mut game, owner, Improvement::Fireplace2);
    let before_food = game.public.players[owner].supply.food;
    let proposed = proposal(&game, Space::SheepMarket).unwrap();
    let Detail::Animals(default_plan) = &proposed.detail else {
        panic!("expected an animal plan, got {:?}", proposed.detail);
    };
    assert_ne!(default_plan, &plan);
    game.apply(Action {
        space: Space::SheepMarket,
        detail: Detail::Animals(plan),
    })
    .unwrap();
    let player = &game.public.players[owner];
    assert_eq!(player.pastures[0].quantity, 1);
    assert_eq!(player.pastures[0].animal, Some(Animal::Sheep));
    assert_eq!(player.farm[12].stable_animal, Some(Animal::Sheep));
    assert_eq!(player.pet, Some(Animal::Sheep));
    assert_eq!(
        player.supply.food,
        before_food + 2,
        "a fireplace cooks one sheep into two food"
    );
    assert_eq!(player.animal_count(Animal::Sheep), 3);
    game.validate().unwrap();
}

#[test]
fn animal_plan_rejects_mixed_species_and_quantity_mismatches_atomically() {
    let (mut game, owner) = farm_with_pasture_and_stable();
    game.public.players[owner].pastures[0].animal = Some(Animal::Boar);
    game.public.players[owner].pastures[0].quantity = 1;
    reveal(&mut game, RoundCard::SheepMarket);
    game.public
        .animal_accumulations
        .insert(Space::SheepMarket, 2);
    let before = game.state_hash();
    let mixed = Action {
        space: Space::SheepMarket,
        detail: Detail::Animals(AnimalPlan {
            animal: Animal::Sheep,
            to_pastures: vec![(0, 2)],
            to_stables: vec![],
            pet: false,
            cook: 0,
            discard: 0,
        }),
    };
    assert!(game.validate_action(&mixed).is_err());
    let short = Action {
        space: Space::SheepMarket,
        detail: Detail::Animals(AnimalPlan {
            animal: Animal::Sheep,
            to_pastures: vec![(0, 1)],
            to_stables: vec![],
            pet: false,
            cook: 0,
            discard: 0,
        }),
    };
    assert!(game.validate_action(&short).is_err());
    assert!(game.apply(mixed).is_err());
    assert!(game.apply(short).is_err());
    assert_eq!(game.state_hash(), before);
    game.validate().unwrap();
}

/* ------------------------------------------------------- baking / buying */

#[test]
fn baking_accepts_a_hearth_first_allocation_and_rejects_repeated_equipment() {
    let mut game = new_game(2, 51, false);
    let owner = actor(&game);
    reveal(&mut game, RoundCard::GrainUtilization);
    give_improvement(&mut game, owner, Improvement::Hearth4);
    give_improvement(&mut game, owner, Improvement::ClayOven);
    game.public.players[owner].supply.grain = 3;
    game.public.players[owner].supply.food = 0;
    let proposed = proposal(&game, Space::GrainUtilization).unwrap();
    let Detail::SowDetailed { baking, .. } = &proposed.detail else {
        panic!("expected sow/bake detail, got {:?}", proposed.detail);
    };
    // The default policy fills the oven first; a human may bake on the hearth
    // first and still use the oven once.
    let human = vec![
        BakeAllocation {
            improvement: Improvement::Hearth4,
            grain: 2,
        },
        BakeAllocation {
            improvement: Improvement::ClayOven,
            grain: 1,
        },
    ];
    assert_ne!(baking, &human);
    game.apply(Action {
        space: Space::GrainUtilization,
        detail: Detail::SowDetailed {
            plantings: vec![],
            baking: human,
        },
    })
    .unwrap();
    let player = &game.public.players[owner];
    assert_eq!(player.supply.grain, 0);
    assert_eq!(
        player.supply.food, 11,
        "2 grain * 3 food + 1 grain * 5 food"
    );
    game.validate().unwrap();

    // Rejected allocations are checked on a fresh copy because the baking
    // action is already spent in this round.
    let mut again = new_game(2, 52, false);
    let owner = actor(&again);
    reveal(&mut again, RoundCard::GrainUtilization);
    give_improvement(&mut again, owner, Improvement::ClayOven);
    again.public.players[owner].supply.grain = 3;
    let before = again.state_hash();
    let repeated = Action {
        space: Space::GrainUtilization,
        detail: Detail::SowDetailed {
            plantings: vec![],
            baking: vec![
                BakeAllocation {
                    improvement: Improvement::ClayOven,
                    grain: 1,
                },
                BakeAllocation {
                    improvement: Improvement::ClayOven,
                    grain: 1,
                },
            ],
        },
    };
    assert!(again.apply(repeated).is_err());
    let over_capacity = Action {
        space: Space::GrainUtilization,
        detail: Detail::SowDetailed {
            plantings: vec![],
            baking: vec![BakeAllocation {
                improvement: Improvement::ClayOven,
                grain: 2,
            }],
        },
    };
    assert!(again.apply(over_capacity).is_err());
    assert_eq!(again.state_hash(), before);
    again.validate().unwrap();
}

#[test]
fn buying_a_hearth_by_payment_while_owning_a_fireplace_is_a_legal_choice() {
    let mut game = new_game(2, 61, false);
    let owner = actor(&game);
    reveal(&mut game, RoundCard::MajorImprovement);
    give_improvement(&mut game, owner, Improvement::Fireplace2);
    game.public.players[owner].supply.clay = 5;
    game.public.players[owner].supply.grain = 2;
    let human = Detail::BuyWithMode {
        improvement: Improvement::Hearth4,
        instant_baking: vec![],
        mode: PurchaseMode::Pay,
    };
    let proposed = proposal(&game, Space::MajorImprovement).unwrap();
    assert_ne!(proposed.detail, human);
    game.apply(Action {
        space: Space::MajorImprovement,
        detail: human,
    })
    .unwrap();
    let player = &game.public.players[owner];
    assert_eq!(
        player.supply.clay, 1,
        "the hearth is paid for, not exchanged"
    );
    assert!(player.improvements.contains(&Improvement::Hearth4));
    assert!(player.improvements.contains(&Improvement::Fireplace2));
    game.validate().unwrap();

    // An instant bake is only allowed for the two ovens.
    let mut game = new_game(2, 62, false);
    let owner = actor(&game);
    reveal(&mut game, RoundCard::MajorImprovement);
    game.public.players[owner].supply.clay = 4;
    game.public.players[owner].supply.grain = 1;
    let before = game.state_hash();
    let illegal = Action {
        space: Space::MajorImprovement,
        detail: Detail::BuyDetailed {
            improvement: Improvement::Hearth4,
            instant_baking: vec![BakeAllocation {
                improvement: Improvement::Hearth4,
                grain: 1,
            }],
        },
    };
    assert!(game.apply(illegal).is_err());
    assert_eq!(game.state_hash(), before);
    game.validate().unwrap();
}

/* ---------------------------------------------------------------- feeding */

/// Force a harvest on the active player's final placement of round 4.
fn harvest_ready_game(seed: u64) -> (Game, usize) {
    let mut game = new_game(2, seed, false);
    while game.public.revealed.len() < 4 {
        let next = game.hidden_round_deck.remove(0);
        game.public.revealed.push(next);
    }
    game.public.round = 4;
    game.public.used = [Space::DayLaborer, Space::Fishing, Space::Forest]
        .into_iter()
        .collect();
    game.public.workers_placed = vec![1, 2];
    game.public.active_player = 0;
    let owner = actor(&game);
    assert!(game.will_harvest_after_work());
    (game, owner)
}

struct GrainFirst;

impl HarvestDetailPolicy for GrainFirst {
    fn plan_feeding(&self, _: &Observation) -> FeedingPlan {
        let mut plan = FeedingPlan::default();
        // Feed from grain first, keeping the stored food.
        plan.priorities.swap(0, 5);
        plan
    }
}

struct DuplicateOrder;

impl HarvestDetailPolicy for DuplicateOrder {
    fn plan_feeding(&self, _: &Observation) -> FeedingPlan {
        let mut plan = FeedingPlan::default();
        plan.priorities[0] = FeedSource::Cattle;
        plan
    }
}

#[test]
fn feeding_plan_order_is_a_human_choice_the_default_policy_never_uses() {
    let (mut game, owner) = harvest_ready_game(71);
    game.public.players[owner].supply.food = 4;
    game.public.players[owner].supply.grain = 4;
    game.apply_with_harvest_policy(
        Action {
            space: Space::ClayPit,
            detail: Detail::None,
        },
        &GrainFirst,
    )
    .unwrap();
    let player = &game.public.players[owner];
    assert_eq!(player.supply.grain, 0, "the chosen order ate grain first");
    assert_eq!(player.supply.food, 4, "stored food was preserved");
    assert_eq!(player.begging, 0);
    game.validate().unwrap();

    // The same state under the default order preserves the grain instead.
    let (mut default, owner) = harvest_ready_game(71);
    default.public.players[owner].supply.food = 4;
    default.public.players[owner].supply.grain = 4;
    default
        .apply(Action {
            space: Space::ClayPit,
            detail: Detail::None,
        })
        .unwrap();
    let player = &default.public.players[owner];
    assert_eq!(player.supply.grain, 4);
    assert_eq!(player.supply.food, 0);
    assert_eq!(player.begging, 0);
}

#[test]
fn an_invalid_feeding_plan_is_rejected_without_finishing_the_move() {
    let (mut game, owner) = harvest_ready_game(72);
    game.public.players[owner].supply.food = 4;
    game.public.players[owner].supply.grain = 4;
    let before = game.state_hash();
    let action = Action {
        space: Space::ClayPit,
        detail: Detail::None,
    };
    assert!(game
        .apply_with_harvest_policy(action, &DuplicateOrder)
        .is_err());
    assert_eq!(game.state_hash(), before, "the placement was rolled back");
    assert!(!game.public.used.contains(&Space::ClayPit));
    assert_eq!(game.public.workers_placed[owner], 1);
    game.validate().unwrap();
}

/* ----------------------------------------------------------- free actions */

#[test]
fn free_actions_accept_cooking_conversion_and_rearrangement() {
    let (mut game, owner) = farm_with_pasture_and_stable();
    give_improvement(&mut game, owner, Improvement::Hearth4);
    game.public.players[owner].supply.vegetable = 2;
    let food = game.public.players[owner].supply.food;
    game.apply_free(
        owner,
        FreeAction::ConvertCrop {
            good: Good::Vegetable,
            quantity: 2,
        },
    )
    .unwrap();
    assert_eq!(game.public.players[owner].supply.food, food + 6);
    assert_eq!(game.public.players[owner].supply.vegetable, 0);

    // A mixed holding of pasture, unfenced stable and pet animals.
    game.public.players[owner].pastures[0].animal = Some(Animal::Sheep);
    game.public.players[owner].pastures[0].quantity = 2;
    game.public.players[owner].farm[12].stable_animal = Some(Animal::Sheep);
    game.public.players[owner].pet = Some(Animal::Sheep);
    game.validate().unwrap();
    let food = game.public.players[owner].supply.food;
    game.apply_free(
        owner,
        FreeAction::CookAnimal {
            animal: Animal::Sheep,
            source: AnimalLocation::Pasture(0),
            quantity: 1,
        },
    )
    .unwrap();
    assert_eq!(game.public.players[owner].supply.food, food + 2);
    assert_eq!(game.public.players[owner].animal_count(Animal::Sheep), 3);
    game.apply_free(
        owner,
        FreeAction::DiscardAnimal {
            animal: Animal::Sheep,
            source: AnimalLocation::Stable(12),
            quantity: 1,
        },
    )
    .unwrap();
    assert_eq!(game.public.players[owner].animal_count(Animal::Sheep), 2);
    // Move the pet sheep into the pasture; the explicit choice must be
    // accepted and conserve the total.
    game.apply_free(
        owner,
        FreeAction::RearrangeAnimals {
            pasture: vec![(0, Some(Animal::Sheep), 2)],
            stables: vec![],
            pet: None,
        },
    )
    .unwrap();
    let player = &game.public.players[owner];
    assert_eq!(player.pastures[0].quantity, 2);
    assert_eq!(player.farm[12].stable_animal, None);
    assert_eq!(player.pet, None);
    assert_eq!(player.animal_count(Animal::Sheep), 2);
    game.validate().unwrap();

    // A rearrangement that invents an animal is illegal and leaves no trace.
    let before = game.state_hash();
    assert!(game
        .apply_free(
            owner,
            FreeAction::RearrangeAnimals {
                pasture: vec![(0, Some(Animal::Sheep), 3)],
                stables: vec![],
                pet: Some(Animal::Sheep),
            },
        )
        .is_err());
    assert!(game
        .apply_free(
            owner,
            FreeAction::CookAnimal {
                animal: Animal::Sheep,
                source: AnimalLocation::Pasture(0),
                quantity: 5,
            },
        )
        .is_err());
    assert_eq!(game.state_hash(), before);
    game.validate().unwrap();
}

/* ---------------------------------------------- save, replay, privacy --- */

/// A seed whose first revealed card is Fencing, so a fencing move is reachable
/// without mutating the state that the replay has to rebuild from the config.
fn natural_fencing_seed() -> u64 {
    (0..2_000u64)
        .find(|&seed| new_game(2, seed, false).public.revealed.first() == Some(&RoundCard::Fencing))
        .expect("some seed reveals Fencing in round 1")
}

#[test]
fn non_default_details_survive_save_load_and_action_only_replay() {
    let config = Config {
        players: 2,
        seed: natural_fencing_seed(),
        additional_action_space_enabled: false,
    };
    let mut game = Game::new(config.clone());
    let owner = actor(&game);
    // Gather wood with real placements, then build two separate one-cell
    // pastures (the default policy only ever builds a single new pasture).
    for _ in 0..40 {
        if actor(&game) == owner
            && game.public.players[owner].supply.wood >= 7
            && game.available_spaces().contains(&Space::Fencing)
        {
            let action = Action {
                space: Space::Fencing,
                detail: Detail::Fence {
                    pastures: vec![vec![1], vec![6]],
                },
            };
            game.apply(action).unwrap();
            break;
        }
        let space = [
            Space::Forest,
            Space::DayLaborer,
            Space::Fishing,
            Space::ClayPit,
            Space::ReedBank,
            Space::GrainSeeds,
        ]
        .into_iter()
        .find(|space| game.available_spaces().contains(space))
        .expect("a simple space is always available");
        let action = proposal(&game, space).unwrap();
        game.apply(action).unwrap();
    }
    assert_eq!(game.public.players[owner].pastures.len(), 2);
    assert!(
        !game
            .log
            .events
            .iter()
            .any(|event| event.kind != "work" && event.kind != "free"),
        "only recorded work/free events exist"
    );

    let saved = game.save_json().unwrap();
    let loaded = Game::load_json(&saved).unwrap();
    assert_eq!(loaded.state_hash(), game.state_hash());
    assert_eq!(
        loaded.public.players[owner].fences,
        game.public.players[owner].fences
    );
    let replayed = Game::replay(config, &game.log.events).unwrap();
    assert_eq!(replayed.state_hash(), game.state_hash());
    game.validate().unwrap();
}

#[test]
fn the_detail_api_does_not_leak_private_state() {
    let game = new_game(2, 82, false);
    let owner = actor(&game);
    for space in game.available_spaces() {
        let hints = game.detail_hints(space);
        assert!(
            hints
                .occupations
                .iter()
                .chain(hints.minors.iter())
                .all(|id| game.cards.hands[owner].contains(id)),
            "card hints for {space:?} must come from the active player's hand"
        );
    }
    let json = serde_json::to_string(&game.observation(owner).unwrap()).unwrap();
    for secret in ["hidden_round_deck", "round_seed", "action_seed", "\"rng\""] {
        assert!(!json.contains(secret), "observation leaked {secret}");
    }
    assert!(!serde_json::to_string(&game.fence_hints())
        .unwrap()
        .contains("opponent"));
}

/* ------------------------------------------------------------ cultivation */

#[test]
fn cultivation_accepts_sowing_without_plowing_and_planting_the_new_field() {
    let mut game = new_game(2, 91, false);
    let owner = actor(&game);
    reveal(&mut game, RoundCard::Cultivation);
    game.public.players[owner].farm[1].tile = empty_field();
    game.public.players[owner].supply.grain = 2;
    let proposed = proposal(&game, Space::Cultivation).unwrap();
    let Detail::Cultivate { plow, plantings } = &proposed.detail else {
        panic!("expected a cultivation detail, got {:?}", proposed.detail);
    };
    assert!(plow.is_some(), "the default policy always plows");
    assert_eq!(plantings.len(), 2);

    // Sowing only, with no plowing, is a legal human choice.
    let mut sow_only = game.clone();
    sow_only
        .apply(Action {
            space: Space::Cultivation,
            detail: Detail::Cultivate {
                plow: None,
                plantings: vec![Plant {
                    cell: 1,
                    crop: Crop::Grain,
                }],
            },
        })
        .unwrap();
    let player = &sow_only.public.players[owner];
    assert_eq!(
        player.farm[1].tile,
        Tile::Field {
            crop: Some(Crop::Grain),
            remaining: 3
        }
    );
    assert_eq!(player.supply.grain, 1);
    assert_eq!(
        player
            .farm
            .iter()
            .filter(|cell| matches!(cell.tile, Tile::Field { .. }))
            .count(),
        1,
        "no field was plowed"
    );
    sow_only.validate().unwrap();

    // Plowing and sowing the same new field in one action is also legal.
    let mut both = game.clone();
    both.apply(Action {
        space: Space::Cultivation,
        detail: Detail::Cultivate {
            plow: Some(2),
            plantings: vec![Plant {
                cell: 2,
                crop: Crop::Grain,
            }],
        },
    })
    .unwrap();
    let player = &both.public.players[owner];
    assert_eq!(
        player.farm[2].tile,
        Tile::Field {
            crop: Some(Crop::Grain),
            remaining: 3
        }
    );
    both.validate().unwrap();

    // Plowing and sowing nothing is rejected by the shared validator.
    let mut empty = game.clone();
    let before = empty.state_hash();
    assert!(empty
        .apply(Action {
            space: Space::Cultivation,
            detail: Detail::Cultivate {
                plow: None,
                plantings: vec![],
            },
        })
        .is_err());
    assert_eq!(empty.state_hash(), before);
}

#[test]
fn farm_expansion_accepts_stables_only_when_a_room_is_unaffordable() {
    let mut game = new_game(2, 92, false);
    let owner = actor(&game);
    game.public.players[owner].supply.wood = 2;
    game.public.players[owner].supply.reed = 0;
    let proposed = proposal(&game, Space::FarmExpansion).unwrap();
    assert_eq!(
        proposed.detail,
        Detail::Build {
            rooms: vec![],
            stables: vec![1],
        }
    );
    let mut chosen = game.clone();
    chosen
        .apply(Action {
            space: Space::FarmExpansion,
            detail: Detail::Build {
                rooms: vec![],
                stables: vec![9],
            },
        })
        .unwrap();
    assert!(chosen.public.players[owner].farm[9].stable);
    assert_eq!(chosen.public.players[owner].supply.wood, 0);
    chosen.validate().unwrap();
}
