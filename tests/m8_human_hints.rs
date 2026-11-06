//! M8-2: staged human-input hints must describe legal options, cover fences,
//! feeding, breeding and free actions, stay read-only, and never read another
//! player's hidden state. Hints never define legality; the engine validator does.
use agricola_re_ai::breeding::BirthDestination;
use agricola_re_ai::cards::{verified_basic_card_set, CardKind};
use agricola_re_ai::engine::{Action, Detail};
use agricola_re_ai::harvest::FeedSource;
use agricola_re_ai::hints;
use agricola_re_ai::model::{
    Animal, Config, Game, Good, Improvement, Pasture, Player, RoundCard, Space, Tile,
};

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

fn fence_action(proposal: &hints::FenceProposal) -> Action {
    Action {
        space: Space::Fencing,
        detail: match &proposal.occupancy {
            Some(occupancy) => Detail::FenceDetailed {
                pastures: proposal.pastures.clone(),
                occupancy: occupancy.clone(),
            },
            None => Detail::Fence {
                pastures: proposal.pastures.clone(),
            },
        },
    }
}

#[test]
fn every_fence_proposal_is_legal_and_multicell_is_offered() {
    let mut game = game_with_fencing();
    let player = game.public.active_player;
    game.public.players[player].supply.wood = 12;
    let hints = game.fence_hints();
    assert_eq!(hints.available_wood, 12);
    assert!(
        hints
            .proposals
            .iter()
            .any(|p| p.kind == "new" && p.pastures[0].len() >= 2),
        "a multi-cell fresh pasture must be offered"
    );
    assert!(!hints.proposals.is_empty());
    for proposal in &hints.proposals {
        game.validate_action(&fence_action(proposal))
            .unwrap_or_else(|error| panic!("hint proposal must be legal: {proposal:?}: {error}"));
    }
}

fn border(i: usize) -> [(u8, u8); 4] {
    let x = (i % 5) as u8;
    let y = (i / 5) as u8;
    let pts = [
        y * 6 + x,
        y * 6 + x + 1,
        (y + 1) * 6 + x,
        (y + 1) * 6 + x + 1,
    ];
    [
        (pts[0], pts[1]),
        (pts[2], pts[3]),
        (pts[0], pts[2]),
        (pts[1], pts[3]),
    ]
}

fn boundary(cells: &[usize]) -> std::collections::BTreeSet<(u8, u8)> {
    let mut edges = std::collections::BTreeSet::new();
    for &cell in cells {
        for edge in border(cell) {
            if !edges.insert(edge) {
                edges.remove(&edge);
            }
        }
    }
    edges
}

#[test]
fn fence_hints_offer_a_new_pasture_adjacent_to_an_existing_one() {
    let mut game = game_with_fencing();
    let player = game.public.active_player;
    game.public.players[player].supply.wood = 12;
    // Seed an existing one-cell pasture at cell 1 without spending the Fencing
    // action, so the space stays available for validation.
    game.public.players[player].farm[1].tile = Tile::Pasture { id: 0 };
    game.public.players[player].pastures.push(Pasture {
        id: 0,
        cells: vec![1],
        animal: None,
        quantity: 0,
    });
    game.public.players[player].fences = boundary(&[1]);
    game.validate().unwrap();
    let hints = game.fence_hints();
    assert!(
        !hints.extendable_cells.is_empty(),
        "cells adjacent to the existing pasture must be listed"
    );
    // A separate new region touching the existing pasture shares its fence and
    // is legal; merging cells into the old pasture would demolish a fence.
    assert!(
        hints.proposals.iter().any(|p| p.kind == "new"),
        "an adjacent new pasture must be offered"
    );
    for proposal in &hints.proposals {
        game.validate_action(&fence_action(proposal))
            .unwrap_or_else(|error| {
                panic!("adjacent proposal must be legal: {proposal:?}: {error}")
            });
    }
}

#[test]
fn feeding_hints_report_owed_food_and_conversion_rates() {
    let mut player = Player::new(0, 5);
    player.family = 4;
    player.newborns = 1;
    player.improvements.push(Improvement::Joinery);
    player.supply.wood = 2;
    player.supply.vegetable = 2;
    player.improvements.push(Improvement::Hearth4);
    player.pet = Some(Animal::Sheep);
    let hints = hints::feeding_hints(&player);
    assert_eq!(hints.owed_food, (4 - 1) * 2 + 1);
    assert_eq!(hints.stored_food, 5);
    let vegetable = hints
        .options
        .iter()
        .find(|option| option.source == FeedSource::Vegetables)
        .unwrap();
    assert_eq!(vegetable.food_each, 3);
    let joinery = hints
        .options
        .iter()
        .find(|option| option.source == FeedSource::Joinery)
        .unwrap();
    assert_eq!(joinery.available, 2);
    let sheep = hints
        .options
        .iter()
        .find(|option| option.source == FeedSource::Sheep)
        .unwrap();
    assert!(sheep.preserves_pair_by_default);
    // A player without a cooking improvement cannot cook animals.
    let plain = Player::new(1, 0);
    assert_eq!(
        hints::feeding_hints(&plain)
            .options
            .iter()
            .find(|option| option.source == FeedSource::Sheep)
            .unwrap()
            .food_each,
        0
    );
    // Workshop processing is a harvest feeding option, not a free action.
    let mut workshopper = Player::new(0, 0);
    workshopper.improvements.push(Improvement::Joinery);
    workshopper.supply.wood = 1;
    assert!(hints::feeding_hints(&workshopper)
        .options
        .iter()
        .any(|option| option.source == FeedSource::Joinery));
}

#[test]
fn breeding_hints_list_eligible_species_and_legal_destinations() {
    let mut player = Player::new(0, 0);
    player.pet = Some(Animal::Sheep);
    player.farm[14].stable = true;
    player.farm[14].stable_animal = Some(Animal::Sheep);
    player.pastures.push(Pasture {
        id: 0,
        cells: vec![13],
        animal: Some(Animal::Boar),
        quantity: 2,
    });
    player.farm[13].tile = Tile::Pasture { id: 0 };
    let hints = hints::breeding_hints(&player);
    let sheep = hints
        .eligible
        .iter()
        .find(|entry| entry.species == Animal::Sheep)
        .unwrap();
    assert!(sheep
        .destinations
        .iter()
        .any(|destination| matches!(destination.destination, BirthDestination::Discard)));
    let boar = hints
        .eligible
        .iter()
        .find(|entry| entry.species == Animal::Boar)
        .unwrap();
    // The single boar pasture is full, so no pasture destination is offered.
    assert!(!boar
        .destinations
        .iter()
        .any(|d| matches!(d.destination, BirthDestination::Pasture(_))));
    assert!(boar
        .destinations
        .iter()
        .any(|d| matches!(d.destination, BirthDestination::Discard)));
}

#[test]
fn free_action_hints_report_only_available_conversions() {
    let mut player = Player::new(0, 0);
    player.supply.vegetable = 1;
    player.improvements.push(Improvement::Hearth4);
    player.pet = Some(Animal::Sheep);
    let hints = hints::free_action_hints(&player);
    assert!(hints
        .convertible_crops
        .iter()
        .any(|(good, available, rate)| *good == Good::Vegetable && *available == 1 && *rate == 3));
    assert!(hints
        .cookable_animals
        .iter()
        .any(|(animal, _, rate)| *animal == Animal::Sheep && *rate == 2));
    assert!(hints
        .cookable_animals
        .iter()
        .all(|(animal, available, _)| *animal != Animal::Boar || *available == 0));

    let plain = Player::new(1, 0);
    assert!(hints::free_action_hints(&plain).cookable_animals.is_empty());
}

#[test]
fn hints_are_read_only() {
    let game = Game::new(Config {
        players: 2,
        seed: 0,
        additional_action_space_enabled: true,
    });
    let before = game.state_hash();
    let _ = game.fence_hints();
    let _ = hints::feeding_hints(&game.public.players[0]);
    let _ = hints::breeding_hints(&game.public.players[0]);
    let _ = hints::free_action_hints(&game.public.players[0]);
    assert_eq!(game.state_hash(), before);
}

#[test]
fn hints_ignore_the_hidden_opponent_hand() {
    let game = Game::with_cards(
        Config {
            players: 2,
            seed: 40,
            additional_action_space_enabled: false,
        },
        verified_basic_card_set(2).unwrap(),
    )
    .unwrap();
    let mut changed = game.clone();
    if let Some(index) = changed.cards.hands[1]
        .iter()
        .position(|id| changed.card_set.definition(id).unwrap().kind == CardKind::MinorImprovement)
    {
        let replacement = changed.cards.undealt.iter().position(|id| {
            changed.card_set.definition(id).unwrap().kind == CardKind::MinorImprovement
        });
        if let Some(replacement) = replacement {
            let old = std::mem::replace(
                &mut changed.cards.hands[1][index],
                changed.cards.undealt[replacement].clone(),
            );
            changed.cards.undealt[replacement] = old;
            changed.validate().unwrap();
        }
    }
    assert_eq!(
        serde_json::to_value(game.observation(0).unwrap()).unwrap(),
        serde_json::to_value(changed.observation(0).unwrap()).unwrap()
    );
    assert_eq!(game.fence_hints(), changed.fence_hints());
    assert_eq!(
        hints::free_action_hints(&game.public.players[0]),
        hints::free_action_hints(&changed.public.players[0])
    );
    assert_eq!(
        hints::feeding_hints(&game.public.players[0]),
        hints::feeding_hints(&changed.public.players[0])
    );
}

#[test]
fn fence_hints_offer_a_legal_split_of_an_empty_pasture() {
    let mut game = game_with_fencing();
    let player = game.public.active_player;
    game.public.players[player].supply.wood = 12;
    game.public.players[player].farm[1].tile = Tile::Pasture { id: 0 };
    game.public.players[player].farm[2].tile = Tile::Pasture { id: 0 };
    game.public.players[player].pastures.push(Pasture {
        id: 0,
        cells: vec![1, 2],
        animal: None,
        quantity: 0,
    });
    game.public.players[player].fences = boundary(&[1, 2]);
    game.validate().unwrap();
    let hints = game.fence_hints();
    assert!(
        hints.proposals.iter().any(|p| p.kind == "split"),
        "a split proposal must be offered for a two-cell pasture"
    );
    for proposal in hints.proposals.iter().filter(|p| p.kind == "split") {
        game.validate_action(&fence_action(proposal))
            .unwrap_or_else(|error| panic!("split must be legal: {proposal:?}: {error}"));
    }
}

#[test]
fn web_view_exposes_fence_and_free_action_hints() {
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
    assert!(view["fence_hints"].is_object());
    assert!(view["free_action_hints"].is_object());
    assert!(view["fence_hints"]["proposals"].is_array());
    assert!(view["free_action_hints"]["held_goods"].is_array());
}
