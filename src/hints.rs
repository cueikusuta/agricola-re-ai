//! Read-only staged hints for human detail input (#109).
//!
//! Hints describe the player's own options and the public board only. They are
//! never a legality definition: `Game::validate_action` stays the single
//! validator, and the UI may submit any detail the engine accepts, including
//! one the hints do not mention. Nothing here reads another player's hand or
//! the hidden round order.
use crate::breeding::BirthDestination;
use crate::engine::{AnimalLocation, PastureOccupancy};
use crate::harvest::FeedSource;
use crate::model::{Animal, Crop, Good, Improvement, Player, Tile, FARM_CELLS};
use serde::{Deserialize, Serialize};

/// Food produced by converting one vegetable, matching rule application.
pub fn vegetable_food(p: &Player) -> u32 {
    if p.improvements
        .iter()
        .any(|i| matches!(i, Improvement::Hearth4 | Improvement::Hearth5))
    {
        3
    } else if p
        .improvements
        .iter()
        .any(|i| matches!(i, Improvement::Fireplace2 | Improvement::Fireplace3))
    {
        2
    } else {
        1
    }
}

/// Food produced by cooking one animal, matching rule application.
pub fn animal_food(p: &Player, animal: Animal) -> Option<u32> {
    if p.improvements
        .iter()
        .any(|i| matches!(i, Improvement::Hearth4 | Improvement::Hearth5))
    {
        Some(match animal {
            Animal::Sheep => 2,
            Animal::Boar => 3,
            Animal::Cattle => 4,
        })
    } else if p
        .improvements
        .iter()
        .any(|i| matches!(i, Improvement::Fireplace2 | Improvement::Fireplace3))
    {
        Some(match animal {
            Animal::Sheep => 2,
            Animal::Boar => 2,
            Animal::Cattle => 3,
        })
    } else {
        None
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FeedOptionHint {
    pub source: FeedSource,
    /// Units the player currently holds of this source.
    pub available: u32,
    /// Food produced per unit, or 1 for stored food and grain.
    pub food_each: u32,
    /// Whether the rule application defers this source while a breeding pair
    /// could still be preserved.
    pub preserves_pair_by_default: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FeedingHints {
    pub owed_food: u32,
    pub stored_food: u32,
    pub preserve_pairs_first: bool,
    pub options: Vec<FeedOptionHint>,
}

/// Feeding options for one player, computed after the field phase.
pub fn feeding_hints(p: &Player) -> FeedingHints {
    let owed = u32::from(p.family - p.newborns) * 2 + u32::from(p.newborns);
    let mut options = vec![FeedOptionHint {
        source: FeedSource::StoredFood,
        available: p.supply.food,
        food_each: 1,
        preserves_pair_by_default: false,
    }];
    for (improvement, good, food_each) in [
        (Improvement::Joinery, Good::Wood, 2),
        (Improvement::Pottery, Good::Clay, 2),
        (Improvement::Basketry, Good::Reed, 3),
    ] {
        if p.improvements.contains(&improvement) {
            options.push(FeedOptionHint {
                source: match improvement {
                    Improvement::Joinery => FeedSource::Joinery,
                    Improvement::Pottery => FeedSource::Pottery,
                    _ => FeedSource::Basketry,
                },
                available: p.supply.get(good),
                food_each,
                preserves_pair_by_default: false,
            });
        }
    }
    options.push(FeedOptionHint {
        source: FeedSource::Vegetables,
        available: p.supply.vegetable,
        food_each: vegetable_food(p),
        preserves_pair_by_default: false,
    });
    options.push(FeedOptionHint {
        source: FeedSource::Grain,
        available: p.supply.grain,
        food_each: 1,
        preserves_pair_by_default: false,
    });
    for animal in [Animal::Sheep, Animal::Boar, Animal::Cattle] {
        let source = match animal {
            Animal::Sheep => FeedSource::Sheep,
            Animal::Boar => FeedSource::Boar,
            Animal::Cattle => FeedSource::Cattle,
        };
        options.push(FeedOptionHint {
            source,
            available: p.animal_count(animal),
            food_each: animal_food(p, animal).unwrap_or(0),
            preserves_pair_by_default: true,
        });
    }
    FeedingHints {
        owed_food: owed,
        stored_food: p.supply.food,
        preserve_pairs_first: true,
        options,
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BirthDestinationHint {
    pub destination: BirthDestination,
    /// Free space for the newborn in this destination (1 for Pet/Discard).
    pub free_space: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SpeciesBreedingHint {
    pub species: Animal,
    pub destinations: Vec<BirthDestinationHint>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BreedingHints {
    pub eligible: Vec<SpeciesBreedingHint>,
    pub rearrangement_available: bool,
}

/// Breeding destinations for one player, computed after feeding.
pub fn breeding_hints(p: &Player) -> BreedingHints {
    let mut eligible = Vec::new();
    for species in [Animal::Sheep, Animal::Boar, Animal::Cattle] {
        if p.animal_count(species) < 2 {
            continue;
        }
        let mut destinations = Vec::new();
        for pasture in &p.pastures {
            if pasture.animal.is_some() && pasture.animal != Some(species) {
                continue;
            }
            let capacity = p.pasture_capacity(pasture);
            if pasture.quantity < capacity {
                destinations.push(BirthDestinationHint {
                    destination: BirthDestination::Pasture(pasture.id),
                    free_space: capacity - pasture.quantity,
                });
            }
        }
        for (index, cell) in p.farm.iter().enumerate() {
            if cell.stable && matches!(cell.tile, Tile::Empty) && cell.stable_animal.is_none() {
                destinations.push(BirthDestinationHint {
                    destination: BirthDestination::Stable(index),
                    free_space: 1,
                });
            }
        }
        if p.pet.is_none() {
            destinations.push(BirthDestinationHint {
                destination: BirthDestination::Pet,
                free_space: 1,
            });
        }
        destinations.push(BirthDestinationHint {
            destination: BirthDestination::Discard,
            free_space: u32::MAX,
        });
        eligible.push(SpeciesBreedingHint {
            species,
            destinations,
        });
    }
    BreedingHints {
        eligible,
        rearrangement_available: !p.pastures.is_empty(),
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FreeActionHints {
    /// Crops that can be converted: source good, held amount, food per unit.
    pub convertible_crops: Vec<(Good, u32, u32)>,
    /// Animals that a cooking improvement can convert: animal, held amount,
    /// food per unit.
    pub cookable_animals: Vec<(Animal, u32, u32)>,
    /// Workshops usable as a free conversion this harvest: improvement, good
    /// paid, held amount, food produced.
    pub workshops: Vec<(Improvement, Good, u32, u32)>,
    /// Every held good and its amount, so a player can return surplus.
    pub held_goods: Vec<(Good, u32)>,
}

/// Free-action options for one player. Legal timing is still enforced by the
/// engine; this only lists what the player currently holds.
pub fn free_action_hints(p: &Player) -> FreeActionHints {
    let mut convertible_crops = vec![(Good::Grain, p.supply.grain, 1)];
    if p.supply.vegetable > 0 || vegetable_food(p) > 1 {
        convertible_crops.push((Good::Vegetable, p.supply.vegetable, vegetable_food(p)));
    }
    let mut cookable_animals = Vec::new();
    for animal in [Animal::Sheep, Animal::Boar, Animal::Cattle] {
        if let Some(food) = animal_food(p, animal) {
            let available = p.animal_count(animal);
            if available > 0 {
                cookable_animals.push((animal, available, food));
            }
        }
    }
    let mut workshops = Vec::new();
    for (improvement, good, food) in [
        (Improvement::Joinery, Good::Wood, 2),
        (Improvement::Pottery, Good::Clay, 2),
        (Improvement::Basketry, Good::Reed, 3),
    ] {
        if p.improvements.contains(&improvement) {
            workshops.push((improvement, good, p.supply.get(good), food));
        }
    }
    let held_goods = [
        Good::Wood,
        Good::Clay,
        Good::Reed,
        Good::Stone,
        Good::Food,
        Good::Grain,
        Good::Vegetable,
    ]
    .into_iter()
    .map(|good| (good, p.supply.get(good)))
    .collect();
    FreeActionHints {
        convertible_crops,
        cookable_animals,
        workshops,
        held_goods,
    }
}

/// A legal fencing construction the human may adopt.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FenceProposal {
    /// Complete new regions for every changed pasture.
    pub pastures: Vec<Vec<usize>>,
    /// Required when an occupied pasture is split or merged across species.
    pub occupancy: Option<Vec<PastureOccupancy>>,
    pub cost_wood: u32,
    /// Why this proposal is offered: "new", "extend", or "split".
    pub kind: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FenceHints {
    pub available_wood: u32,
    pub free_fence_wood: u32,
    pub remaining_segments: usize,
    /// Empty cells adjacent to an existing pasture (extension targets).
    pub extendable_cells: Vec<usize>,
    pub proposals: Vec<FenceProposal>,
}

/// The usable AnimalLocation sources for the free `CookAnimal`/`DiscardAnimal`
/// actions, for the UI's staged animal-source selector.
pub fn animal_locations(p: &Player, animal: Animal) -> Vec<(AnimalLocation, u32)> {
    let mut out = Vec::new();
    if p.pet == Some(animal) {
        out.push((AnimalLocation::Pet, 1));
    }
    for pasture in &p.pastures {
        if pasture.animal == Some(animal) && pasture.quantity > 0 {
            out.push((AnimalLocation::Pasture(pasture.id), pasture.quantity));
        }
    }
    for (index, cell) in p.farm.iter().enumerate() {
        if cell.stable_animal == Some(animal) {
            out.push((AnimalLocation::Stable(index), 1));
        }
    }
    out
}

/// Empty cells of the farm, used by the UI to bound manual entry.
pub fn empty_cells(p: &Player) -> Vec<usize> {
    (0..FARM_CELLS)
        .filter(|&i| matches!(p.farm[i].tile, Tile::Empty) && !p.farm[i].stable)
        .collect()
}

/// Sown fields (no crop yet) that can receive a grain/vegetable seed.
pub fn empty_fields(p: &Player) -> Vec<usize> {
    (0..FARM_CELLS)
        .filter(|&i| matches!(p.farm[i].tile, Tile::Field { crop: None, .. }))
        .collect()
}

/// Fields currently carrying the given crop.
pub fn cropped_fields(p: &Player, crop: Crop) -> Vec<usize> {
    (0..FARM_CELLS)
        .filter(
            |&i| matches!(p.farm[i].tile, Tile::Field { crop: Some(found), .. } if found == crop),
        )
        .collect()
}
