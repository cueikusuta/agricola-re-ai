//! M2: public-observation-only evaluation and one-step action estimates.
//! These projections rank actions; the rules engine remains the only authority.
use crate::engine::{Action, Detail};
use crate::model::*;
use crate::policy::{weight, DetailPolicy, SimpleDetails, SpacePolicy};
use crate::score::score;
use rand::seq::SliceRandom;
use rand_chacha::ChaCha8Rng;
use std::collections::BTreeSet;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Evaluation {
    pub scoring: f64,
    pub food_safety: f64,
    pub family_growth: f64,
    pub housing: f64,
    pub farm: f64,
    pub resources: f64,
}

impl Evaluation {
    pub fn total(self) -> f64 {
        self.scoring
            + self.food_safety
            + self.family_growth
            + self.housing
            + self.farm
            + self.resources
    }
}

fn marginal(n: u32, threshold: u32, first: f64, later: f64) -> f64 {
    n.min(threshold) as f64 * first + n.saturating_sub(threshold) as f64 * later
}

/// Food conversion factors match the M1 baking rules in engine::bake_allocated.
fn baking_rate(improvement: Improvement) -> Option<u32> {
    match improvement {
        Improvement::Fireplace2 | Improvement::Fireplace3 => Some(2),
        Improvement::Hearth4 | Improvement::Hearth5 => Some(3),
        Improvement::ClayOven => Some(5),
        Improvement::StoneOven => Some(4),
        _ => None,
    }
}

/// Food conversion factors match engine::cook_value / harvest::animal_food.
fn animal_cooking_rate(p: &Player, animal: Animal) -> Option<u32> {
    if p.improvements
        .iter()
        .any(|i| matches!(i, Improvement::Hearth4 | Improvement::Hearth5))
    {
        Some(match animal {
            Animal::Sheep => 2,
            Animal::Boar => 3,
            Animal::Cattle => 4,
        })
    } else if p.improvements
        .iter()
        .any(|i| matches!(i, Improvement::Fireplace2 | Improvement::Fireplace3))
    {
        Some(match animal {
            Animal::Sheep | Animal::Boar => 2,
            Animal::Cattle => 3,
        })
    } else {
        None
    }
}

/// Upper bound on meals available for the next harvest, including one crop per
/// planted field and resources that the harvest policy can convert. This is a
/// safety estimate, not a simulation of the feeding order or future production.
fn next_harvest_food_capacity(p: &Player) -> u32 {
    let mut grain = p.supply.grain;
    let mut vegetables = p.supply.vegetable;
    for cell in &p.farm {
        match cell.tile {
            Tile::Field {
                crop: Some(Crop::Grain),
                remaining: n,
            } if n > 0 => grain = grain.saturating_add(1),
            Tile::Field {
                crop: Some(Crop::Vegetable),
                remaining: n,
            } if n > 0 => vegetables = vegetables.saturating_add(1),
            _ => {}
        }
    }
    let vegetable_rate = if p
        .improvements
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
    };
    let mut available = p
        .supply
        .food
        .saturating_add(grain)
        .saturating_add(vegetables.saturating_mul(vegetable_rate));
    for (card, stock, yield_food) in [
        (Improvement::Joinery, p.supply.wood, 2),
        (Improvement::Pottery, p.supply.clay, 2),
        (Improvement::Basketry, p.supply.reed, 3),
    ] {
        if p.improvements.contains(&card) && stock > 0 {
            available = available.saturating_add(yield_food);
        }
    }
    for animal in [Animal::Sheep, Animal::Boar, Animal::Cattle] {
        if let Some(rate) = animal_cooking_rate(p, animal) {
            available = available.saturating_add(p.animal_count(animal).saturating_mul(rate));
        }
    }
    available
}

/// A transparent, deterministic heuristic. It depends on public data only;
/// its units are heuristic utility, not victory probabilities.
pub fn evaluate_player(p: &Player, round: u8) -> Evaluation {
    let remaining = (15u8.saturating_sub(round)) as f64;
    let next_harvest = HARVEST_ROUNDS
        .iter()
        .copied()
        .find(|&r| r >= round)
        .unwrap_or(14);
    let distance = next_harvest.saturating_sub(round) as f64;
    let urgency = 1.0 / (1.0 + 0.45 * distance);
    let required = (2 * i32::from(p.family) - i32::from(p.newborns)).max(0) as f64;
    let usable_food = f64::from(next_harvest_food_capacity(p));
    let shortage = (required - usable_food).max(0.0);
    let safety = -shortage * 10.0 * urgency
        + usable_food.min(required) * 1.7 * urgency
        + (f64::from(p.supply.food) - required).max(0.0) * 0.35;
    let unused_rooms = p.rooms().saturating_sub(p.family as usize) as f64;
    let housing = unused_rooms * if p.family < 5 { 12.0 + remaining } else { 2.0 };
    let growing = (p.family.saturating_sub(2)) as f64 * 3.0 * remaining;
    let planted = p
        .farm
        .iter()
        .map(|cell| match cell.tile {
            Tile::Field {
                crop: Some(Crop::Grain),
                remaining: n,
            } => f64::from(n) * 1.5,
            Tile::Field {
                crop: Some(Crop::Vegetable),
                remaining: n,
            } => f64::from(n) * 2.0,
            _ => 0.0,
        })
        .sum::<f64>();
    let pasture_capacity = p
        .pastures
        .iter()
        .map(|pasture| {
            let spare = p.pasture_capacity(pasture).saturating_sub(pasture.quantity);
            f64::from(spare.min(4)) * 0.45
        })
        .sum::<f64>();
    let cookers = p.improvements.iter().filter(|i| i.is_cooking()).count();
    let cooking = if cookers > 0 {
        7.0 + remaining * 0.5
    } else {
        0.0
    };
    // A well delivers one food at the start of each of the next five rounds.
    let future_well = if p.improvements.contains(&Improvement::Well) {
        f64::from(14u8.saturating_sub(round).min(5)) * 3.0
    } else {
        0.0
    };
    let farm = planted + pasture_capacity + cooking + future_well;
    let wood = marginal(
        p.supply.wood,
        if p.family < 5 && p.rooms() <= p.family as usize && round <= 9 {
            5
        } else {
            8
        },
        if round <= 10 { 3.0 } else { 1.6 },
        0.8,
    );
    let reed = marginal(p.supply.reed, 2, if round <= 10 { 4.5 } else { 1.4 }, 0.9);
    let clay = marginal(p.supply.clay, 5, 2.1, 0.7);
    let stone = marginal(p.supply.stone, 3, if round >= 7 { 3.3 } else { 2.1 }, 1.2);
    let grain = marginal(p.supply.grain, 1, 8.0, 2.0);
    let vegetables = marginal(p.supply.vegetable, 1, 7.0, 2.0);
    let resources = wood + reed + clay + stone + grain + vegetables;
    Evaluation {
        scoring: f64::from(score(p).total) * 3.0,
        food_safety: safety,
        family_growth: growing,
        housing,
        farm,
        resources,
    }
}

pub fn evaluate_observation(obs: &Observation, player_id: usize) -> Option<Evaluation> {
    obs.public
        .players
        .get(player_id)
        .map(|p| evaluate_player(p, obs.public.round))
}

fn add_accumulation(obs: &Observation, p: &mut Player, space: Space) {
    if let Some((good, _)) = space.accumulation() {
        p.supply.gain(
            good,
            obs.public.accumulations.get(&space).copied().unwrap_or(0),
        );
    }
}

fn sow(p: &mut Player, plantings: &[crate::engine::Plant]) {
    for planting in plantings {
        let good = match planting.crop {
            Crop::Grain => Good::Grain,
            Crop::Vegetable => Good::Vegetable,
        };
        if p.supply.get(good) == 0 {
            continue;
        }
        p.supply.set(good, p.supply.get(good) - 1);
        p.farm[planting.cell].tile = Tile::Field {
            crop: Some(planting.crop),
            remaining: if planting.crop == Crop::Grain { 3 } else { 2 },
        };
    }
}

/// Same perimeter representation as engine::boundary. The symmetric
/// difference cancels edges shared by cells in the same pasture.
fn pasture_boundary(cells: &[usize]) -> BTreeSet<(u8, u8)> {
    let mut edges = BTreeSet::new();
    for &cell in cells {
        let x = (cell % 5) as u8;
        let y = (cell / 5) as u8;
        let a = y * 6 + x;
        let b = a + 1;
        let c = a + 6;
        let d = c + 1;
        for edge in [(a, b), (a, c), (b, d), (c, d)] {
            if !edges.insert(edge) {
                edges.remove(&edge);
            }
        }
    }
    edges
}

/// Project exact fence additions, affected pasture regions and animal
/// relocation for the legal details supplied by SimpleDetails.
fn fence(
    p: &mut Player,
    groups: &[Vec<usize>],
    occupancy: Option<&[crate::engine::PastureOccupancy]>,
) {
    let affected = groups
        .iter()
        .flatten()
        .filter_map(|&cell| match p.farm[cell].tile {
            Tile::Pasture { id } => Some(id),
            _ => None,
        })
        .collect::<BTreeSet<_>>();
    let mut edges = BTreeSet::new();
    for previous in p.pastures.iter().filter(|old| !affected.contains(&old.id)) {
        edges.extend(pasture_boundary(&previous.cells));
    }
    for group in groups {
        edges.extend(pasture_boundary(group));
    }
    let added = edges.difference(&p.fences).count() as u32;
    p.supply.wood = p.supply.wood.saturating_sub(added);

    let mut next_id = p.pastures.iter().map(|old| old.id).max().map_or(0, |n| n + 1);
    let mut assigned_ids = BTreeSet::new();
    let mut changed = Vec::new();
    for (ix, group) in groups.iter().enumerate() {
        let old_ids = group
            .iter()
            .filter_map(|&cell| match p.farm[cell].tile {
                Tile::Pasture { id } => Some(id),
                _ => None,
            })
            .collect::<BTreeSet<_>>();
        let id = if old_ids.len() == 1
            && !assigned_ids.contains(old_ids.iter().next().expect("one id"))
        {
            *old_ids.iter().next().expect("one id")
        } else {
            let id = next_id;
            next_id += 1;
            id
        };
        assigned_ids.insert(id);
        let (animal, quantity) = if let Some(values) = occupancy {
            (values[ix].animal, values[ix].quantity)
        } else {
            let mut animal = None;
            let mut quantity = 0;
            for old in &p.pastures {
                if old.quantity > 0 && old.cells.iter().all(|cell| group.contains(cell)) {
                    animal = old.animal;
                    quantity += old.quantity;
                }
            }
            for &cell in group {
                if let Some(species) = p.farm[cell].stable_animal {
                    animal = Some(species);
                    quantity += 1;
                }
            }
            (animal, quantity)
        };
        changed.push(Pasture {
            id,
            cells: group.clone(),
            animal,
            quantity,
        });
        for &cell in group {
            p.farm[cell].tile = Tile::Pasture { id };
            p.farm[cell].stable_animal = None;
        }
    }
    p.pastures.retain(|old| !affected.contains(&old.id));
    p.pastures.extend(changed);
    p.fences = edges;
}

/// Approximate only the immediate *public* effects of an action.
/// No hidden deck, RNG, or future round information is sampled.
fn project_action(obs: &Observation, action: &Action) -> Option<Player> {
    let mut p = obs.public.players.get(obs.player_id)?.clone();
    match (&action.space, &action.detail) {
        (
            Space::Forest
            | Space::ClayPit
            | Space::ReedBank
            | Space::Fishing
            | Space::MeetingPlace
            | Space::WesternQuarry
            | Space::EasternQuarry
            | Space::Copse,
            Detail::None,
        ) => add_accumulation(obs, &mut p, action.space),
        (Space::DayLaborer, Detail::None) => p.supply.food += 2,
        (Space::GrainSeeds, Detail::None) => p.supply.grain += 1,
        (Space::VegetableSeeds, Detail::None) => p.supply.vegetable += 1,
        (Space::ResourceMarket, Detail::None) => {
            p.supply.stone += 1;
            p.supply.food += 1;
        }
        (
            Space::FamilyGrowthWithRoom | Space::ModestFamilyGrowth | Space::FamilyGrowthNoRoom,
            Detail::None,
        ) => {
            p.family += 1;
            p.newborns += 1;
        }
        (Space::Farmland, Detail::Plow { cell }) => {
            p.farm[*cell].tile = Tile::Field {
                crop: None,
                remaining: 0,
            };
        }
        (Space::FarmExpansion, Detail::Build { rooms, stables }) => {
            let room_good = match p.house {
                House::Wood => Good::Wood,
                House::Clay => Good::Clay,
                House::Stone => Good::Stone,
            };
            for &cell in rooms {
                p.farm[cell].tile = Tile::Room;
                let n = p.supply.get(room_good);
                p.supply.set(room_good, n.saturating_sub(5));
                p.supply.reed = p.supply.reed.saturating_sub(2);
            }
            for &cell in stables {
                p.farm[cell].stable = true;
                p.supply.wood = p.supply.wood.saturating_sub(2);
            }
        }
        (Space::Fencing, Detail::Fence { pastures }) => fence(&mut p, pastures, None),
        (
            Space::Fencing,
            Detail::FenceDetailed {
                pastures,
                occupancy,
            },
        ) => fence(&mut p, pastures, Some(occupancy)),
        (Space::GrainUtilization, Detail::SowDetailed { plantings, baking }) => {
            sow(&mut p, plantings);
            for allocation in baking {
                p.supply.grain = p.supply.grain.saturating_sub(allocation.grain);
                p.supply.food += allocation.grain * baking_rate(allocation.improvement)?;
            }
        }
        (Space::Cultivation, Detail::Cultivate { plow, plantings }) => {
            if let Some(cell) = plow {
                p.farm[*cell].tile = Tile::Field {
                    crop: None,
                    remaining: 0,
                };
            }
            sow(&mut p, plantings);
        }
        (Space::MajorImprovement, Detail::Buy { improvement, .. }) => {
            // Buy with PurchaseMode::Auto: a Hearth exchanges an owned Fireplace
            // without paying the printed cost (engine::buy_improvement).
            let exchange = if matches!(improvement, Improvement::Hearth4 | Improvement::Hearth5) {
                p.improvements
                    .iter()
                    .position(|card| matches!(card, Improvement::Fireplace2 | Improvement::Fireplace3))
            } else {
                None
            };
            if let Some(index) = exchange {
                p.improvements.remove(index);
            } else {
                for (good, n) in improvement.cost() {
                    let stock = p.supply.get(good);
                    p.supply.set(good, stock.saturating_sub(n));
                }
            }
            p.improvements.push(*improvement);
        }
        (Space::HouseRedevelopment | Space::FarmRedevelopment, Detail::Renovate { .. }) => {
            let (new_house, good) = match p.house {
                House::Wood => (House::Clay, Good::Clay),
                House::Clay => (House::Stone, Good::Stone),
                House::Stone => return None,
            };
            p.supply
                .set(good, p.supply.get(good).saturating_sub(p.rooms() as u32));
            p.supply.reed = p.supply.reed.saturating_sub(1);
            p.house = new_house;
        }
        (
            Space::SheepMarket | Space::PigMarket | Space::CattleMarket | Space::AnimalMarket,
            Detail::Animals(plan),
        ) => {
            if action.space == Space::AnimalMarket {
                match plan.animal {
                    Animal::Sheep => p.supply.food += 1,
                    Animal::Cattle => p.supply.food = p.supply.food.saturating_sub(1),
                    Animal::Boar => {}
                }
            }
            if plan.pet {
                p.pet = Some(plan.animal);
            }
            for &(id, quantity) in &plan.to_pastures {
                if let Some(pasture) = p.pastures.iter_mut().find(|x| x.id == id) {
                    pasture.animal = Some(plan.animal);
                    pasture.quantity += quantity;
                }
            }
            for &cell in &plan.to_stables {
                p.farm[cell].stable_animal = Some(plan.animal);
            }
            if plan.cook > 0 {
                p.supply.food += plan.cook * animal_cooking_rate(&p, plan.animal)?;
            }
        }
        _ => return None,
    }
    Some(p)
}

/// Estimated change in the public-state evaluation after taking a legal space.
/// The projection is advisory; legality and consequences come from Game::apply.
pub fn action_value(obs: &Observation, space: Space) -> Option<f64> {
    let action = SimpleDetails.produce(obs, space)?;
    let before = evaluate_observation(obs, obs.player_id)?.total();
    let after = project_action(obs, &action)?;
    let mut value = evaluate_player(&after, obs.public.round).total() - before;
    if space == Space::MeetingPlace && obs.public.start_player != obs.player_id {
        value += if obs.public.round < 10 { 4.0 } else { 1.0 };
    }
    if space == Space::Fencing && obs.public.round <= 12 {
        value += 2.0;
    }
    Some(value)
}


#[cfg(test)]
mod projection_tests {
    use super::*;
    use crate::engine::{AnimalPlan, BakeAllocation, PastureOccupancy};

    fn observed(update: impl FnOnce(&mut Player)) -> Observation {
        let game = Game::new(Config::default());
        let mut obs = game.observation(game.public.active_player).unwrap();
        update(&mut obs.public.players[obs.player_id]);
        obs
    }

    #[test]
    fn baking_uses_each_improvements_actual_food_rate() {
        for (improvement, expected_food) in [
            (Improvement::Fireplace2, 2),
            (Improvement::Hearth4, 3),
            (Improvement::ClayOven, 5),
            (Improvement::StoneOven, 4),
        ] {
            let obs = observed(|p| {
                p.improvements.push(improvement);
                p.supply.grain = 1;
            });
            let initial = obs.public.players[obs.player_id].supply.food;
            let action = Action {
                space: Space::GrainUtilization,
                detail: Detail::SowDetailed {
                    plantings: vec![],
                    baking: vec![BakeAllocation {
                        improvement,
                        grain: 1,
                    }],
                },
            };
            let next = project_action(&obs, &action).unwrap();
            assert_eq!(next.supply.food, initial + expected_food, "{improvement:?}");
            assert_eq!(next.supply.grain, 0);
        }
    }

    #[test]
    fn cooking_animals_uses_species_specific_rates() {
        for (improvement, animal, rate) in [
            (Improvement::Fireplace2, Animal::Sheep, 2),
            (Improvement::Fireplace2, Animal::Boar, 2),
            (Improvement::Fireplace2, Animal::Cattle, 3),
            (Improvement::Hearth4, Animal::Sheep, 2),
            (Improvement::Hearth4, Animal::Boar, 3),
            (Improvement::Hearth4, Animal::Cattle, 4),
        ] {
            let obs = observed(|p| p.improvements.push(improvement));
            let initial = obs.public.players[obs.player_id].supply.food;
            let mut plan = AnimalPlan::discard_all(animal, 0);
            plan.cook = 2;
            let action = Action {
                space: match animal {
                    Animal::Sheep => Space::SheepMarket,
                    Animal::Boar => Space::PigMarket,
                    Animal::Cattle => Space::CattleMarket,
                },
                detail: Detail::Animals(plan),
            };
            let next = project_action(&obs, &action).unwrap();
            assert_eq!(next.supply.food, initial + 2 * rate, "{animal:?}/{improvement:?}");
        }
    }

    #[test]
    fn hearth_exchange_removes_fireplace_without_spending_clay() {
        let obs = observed(|p| {
            p.supply.clay = 0;
            p.improvements.push(Improvement::Fireplace2);
        });
        let action = Action {
            space: Space::MajorImprovement,
            detail: Detail::Buy {
                improvement: Improvement::Hearth4,
                instant_bake: 0,
            },
        };
        let next = project_action(&obs, &action).unwrap();
        assert_eq!(next.improvements, vec![Improvement::Hearth4]);
        assert_eq!(next.supply.clay, 0);
    }

    #[test]
    fn food_safety_includes_edible_grain_and_next_field_harvest() {
        let mut p = Player::new(0, 0);
        let starting = evaluate_player(&p, 14).food_safety;
        p.supply.grain = 4;
        let grain_safe = evaluate_player(&p, 14).food_safety;
        assert!(grain_safe > starting);
        p.supply.grain = 0;
        for cell in [1, 2, 3, 4] {
            p.farm[cell].tile = Tile::Field {
                crop: Some(Crop::Grain),
                remaining: 2,
            };
        }
        assert_eq!(evaluate_player(&p, 14).food_safety, grain_safe);

        let mut vegetables = Player::new(0, 0);
        vegetables.improvements.push(Improvement::Hearth4);
        vegetables.supply.vegetable = 2;
        assert!(evaluate_player(&vegetables, 14).food_safety > starting);
    }

    #[test]
    fn well_future_food_diminishes_by_last_round() {
        let base = Player::new(0, 4);
        let mut improved = base.clone();
        improved.improvements.push(Improvement::Well);
        let early_gain =
            evaluate_player(&improved, 3).total() - evaluate_player(&base, 3).total();
        let late_gain =
            evaluate_player(&improved, 14).total() - evaluate_player(&base, 14).total();
        assert!(early_gain > late_gain + 10.0);
    }

    #[test]
    fn internal_fence_split_tracks_new_segment_and_preserves_animals() {
        let obs = observed(|p| {
            p.supply.wood = 2;
            p.farm[1].tile = Tile::Pasture { id: 0 };
            p.farm[2].tile = Tile::Pasture { id: 0 };
            p.pastures.push(Pasture {
                id: 0,
                cells: vec![1, 2],
                animal: Some(Animal::Sheep),
                quantity: 2,
            });
            p.fences = pasture_boundary(&[1, 2]);
        });
        let initial = &obs.public.players[obs.player_id];
        let action = Action {
            space: Space::Fencing,
            detail: Detail::FenceDetailed {
                pastures: vec![vec![1], vec![2]],
                occupancy: vec![
                    PastureOccupancy {
                        animal: Some(Animal::Sheep),
                        quantity: 2,
                    },
                    PastureOccupancy {
                        animal: None,
                        quantity: 0,
                    },
                ],
            },
        };
        let next = project_action(&obs, &action).unwrap();
        assert_eq!(next.pastures.len(), 2);
        assert_eq!(next.fences.len(), initial.fences.len() + 1);
        assert_eq!(next.supply.wood, initial.supply.wood - 1);
        assert_eq!(next.animal_count(Animal::Sheep), 2);
    }
}

/// Calibration-friendly blend of the M1 tactical prior and M2 state evaluation.
#[derive(Clone, Copy, Debug)]
pub struct WeightedEvaluationPolicy {
    pub prior_weight: f64,
    pub state_weight: f64,
}

impl SpacePolicy for WeightedEvaluationPolicy {
    fn select(&self, obs: &Observation, legal: &[Space], rng: &mut ChaCha8Rng) -> Option<Space> {
        let mut best = f64::NEG_INFINITY;
        let mut candidates = Vec::new();
        for &space in legal {
            let Some(state_delta) = action_value(obs, space) else {
                continue;
            };
            let value =
                self.state_weight * state_delta + self.prior_weight * f64::from(weight(obs, space));
            if value > best + 1e-9 {
                best = value;
                candidates.clear();
                candidates.push(space);
            } else if (value - best).abs() <= 1e-9 {
                candidates.push(space);
            }
        }
        candidates.choose(rng).copied()
    }
}

/// Default M2 policy, with reproducible, fixed calibration parameters.
pub struct EvaluationPolicy;

impl SpacePolicy for EvaluationPolicy {
    fn select(&self, obs: &Observation, legal: &[Space], rng: &mut ChaCha8Rng) -> Option<Space> {
        WeightedEvaluationPolicy {
            prior_weight: 0.5,
            state_weight: 2.0,
        }
        .select(obs, legal, rng)
    }
}
