//! M2: public-observation-only evaluation and one-step action estimates.
//! These projections rank actions; the rules engine remains the only authority.
use crate::engine::{Action, Detail};
use crate::model::*;
use crate::policy::{weight, DetailPolicy, SimpleDetails, SpacePolicy};
use crate::score::score;
use rand::seq::SliceRandom;
use rand_chacha::ChaCha8Rng;

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
    let required = (2 * i32::from(p.family) - i32::from(p.newborns)).max(0);
    let food = p.supply.food as i32;
    let shortage = (required - food).max(0);
    let safety = -shortage as f64 * 10.0 * urgency
        + food.min(required) as f64 * 1.7 * urgency
        + (food - required).max(0) as f64 * 0.35;
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
    let farm = planted + pasture_capacity + cooking;
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

fn fence(p: &mut Player, groups: &[Vec<usize>]) {
    for group in groups {
        let id = p
            .pastures
            .iter()
            .map(|pasture| pasture.id)
            .max()
            .unwrap_or(0)
            .saturating_add(1);
        let new_cells = group
            .iter()
            .filter(|&&cell| matches!(p.farm[cell].tile, Tile::Empty))
            .copied()
            .collect::<Vec<_>>();
        if new_cells.is_empty() {
            continue;
        }
        for &cell in &new_cells {
            p.farm[cell].tile = Tile::Pasture { id };
        }
        p.pastures.push(Pasture {
            id,
            cells: new_cells.clone(),
            animal: None,
            quantity: 0,
        });
        p.supply.wood = p
            .supply
            .wood
            .saturating_sub((2 * new_cells.len() + 2) as u32);
    }
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
        (Space::Fencing, Detail::Fence { pastures }) => fence(&mut p, pastures),
        (Space::Fencing, Detail::FenceDetailed { pastures, .. }) => {
            fence(&mut p, pastures);
        }
        (Space::GrainUtilization, Detail::SowDetailed { plantings, baking }) => {
            sow(&mut p, plantings);
            for allocation in baking {
                p.supply.grain = p.supply.grain.saturating_sub(allocation.grain);
                p.supply.food += allocation.grain * 2;
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
            for (good, n) in improvement.cost() {
                let stock = p.supply.get(good);
                p.supply.set(good, stock.saturating_sub(n));
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
            p.supply.food += 2 * plan.cook;
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
