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
    } else if p
        .improvements
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

/// Which calibrated feature set an evaluation call must use.
///
/// `Baseline` is the M2 calibration and must stay byte-for-byte identical: every
/// existing policy, test and replay depends on it. A variant is only reachable
/// through an explicitly selected policy (`VariantEvaluationPolicy`), so
/// comparing variants cannot silently change the baseline AI.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum EvaluationVariant {
    #[default]
    Baseline,
    /// #106 candidate: the scoring table values grain and vegetables identically
    /// (both give 0..4 points for 0..4 units, and a missing category is -1), and
    /// vegetables can be eaten without baking. The baseline nevertheless prices
    /// the first vegetable below the first grain, so taking `VegetableSeeds` is
    /// dominated. This variant prices them equally. **Measured worse in 2-player
    /// games** (`docs/m13-eval-veg-2026-11-08.md`); kept only as a control.
    VegetableFirst,
    /// #106 candidate 1: require one extra food before the next harvest. Aimed at
    /// the `begged` correlate (4-player non-first rate 18%).
    FoodBuffer,
    /// #106 candidate 2: raise the first vegetable only when the state can afford
    /// it (a cooking improvement exists, or food is already sufficient). The
    /// unconditional variant above lost to the baseline, so this isolates the
    /// condition instead of the price.
    VegetableConditional,
    /// #106 candidate 3: value rooms and grown family more while the family can
    /// still grow. Aimed at the `late_third_worker` correlate (−0.20 in 2-player).
    GrowthRoomPriority,
}

/// A transparent, deterministic heuristic. It depends on public data only;
/// its units are heuristic utility, not victory probabilities.
pub fn evaluate_player(p: &Player, round: u8) -> Evaluation {
    evaluate_player_with(p, round, EvaluationVariant::Baseline)
}

/// Same heuristic with an explicit feature variant (see [`EvaluationVariant`]).
pub fn evaluate_player_with(p: &Player, round: u8, variant: EvaluationVariant) -> Evaluation {
    let remaining = (15u8.saturating_sub(round)) as f64;
    let next_harvest = HARVEST_ROUNDS
        .iter()
        .copied()
        .find(|&r| r >= round)
        .unwrap_or(14);
    let distance = next_harvest.saturating_sub(round) as f64;
    let urgency = 1.0 / (1.0 + 0.45 * distance);
    let food_buffer = match variant {
        EvaluationVariant::FoodBuffer => 1.0,
        _ => 0.0,
    };
    let required = (2 * i32::from(p.family) - i32::from(p.newborns)).max(0) as f64 + food_buffer;
    let usable_food = f64::from(next_harvest_food_capacity(p));
    let shortage = (required - usable_food).max(0.0);
    let safety = -shortage * 10.0 * urgency
        + usable_food.min(required) * 1.7 * urgency
        + (f64::from(p.supply.food) - required).max(0.0) * 0.35;
    let unused_rooms = p.rooms().saturating_sub(p.family as usize) as f64;
    let room_value = match variant {
        EvaluationVariant::GrowthRoomPriority => 18.0 + 1.5 * remaining,
        _ => 12.0 + remaining,
    };
    let housing = unused_rooms * if p.family < 5 { room_value } else { 2.0 };
    // Family members added during the current round can only act from the next round.
    let future_action_rounds = f64::from(14u8.saturating_sub(round));
    let growth_value = match variant {
        EvaluationVariant::GrowthRoomPriority => 3.5,
        _ => 3.0,
    };
    let growing = f64::from(p.family.saturating_sub(2)) * growth_value * future_action_rounds;
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
    let first_vegetable = match variant {
        EvaluationVariant::VegetableFirst => 8.0,
        // Raise the price only when the state can carry an extra crop: a cooking
        // improvement converts it to food, or food is already sufficient.
        EvaluationVariant::VegetableConditional => {
            let has_cooking = p.improvements.iter().any(|i| i.is_cooking());
            if has_cooking || usable_food >= required {
                8.0
            } else {
                7.0
            }
        }
        _ => 7.0,
    };
    let vegetables = marginal(p.supply.vegetable, 1, first_vegetable, 2.0);
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
    p.supply.wood = p
        .supply
        .wood
        .saturating_sub(added.saturating_sub(p.free_fence_wood));

    let mut next_id = p
        .pastures
        .iter()
        .map(|old| old.id)
        .max()
        .map_or(0, |n| n + 1);
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

/// Apply a card to the observer's own projected state. The hand and card
/// definitions are included in Observation; no opponent hand is consulted.
fn project_play_card(
    obs: &Observation,
    player: &mut Player,
    id: &str,
    kind: crate::cards::CardKind,
    fee: u32,
) -> Option<()> {
    let set = crate::cards::CardSet {
        definitions: obs.card_definitions.clone(),
    };
    let mut hand = obs.hand.clone();
    crate::cards::play(&set, &mut hand, player, id, kind, fee).ok()?;
    Some(())
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
            | Space::Copse
            | Space::Grove
            | Space::Hollow
            | Space::TravelingPlayers,
            Detail::None,
        ) => add_accumulation(obs, &mut p, action.space),
        (Space::Lessons | Space::ExtraLessons, Detail::PlayOccupation { card_id }) => {
            let count = p
                .played_cards
                .iter()
                .filter(|id| {
                    obs.card_definitions.iter().any(|card| {
                        card.id == **id && card.kind == crate::cards::CardKind::Occupation
                    })
                })
                .count();
            let fee = if action.space == Space::Lessons {
                u32::from(count > 0)
            } else if obs.public.players.len() == 3 || count >= 2 {
                2
            } else {
                1
            };
            project_play_card(
                obs,
                &mut p,
                card_id,
                crate::cards::CardKind::Occupation,
                fee,
            )?;
        }
        (Space::MeetingPlace | Space::MajorImprovement, Detail::PlayMinor { card_id }) => {
            project_play_card(
                obs,
                &mut p,
                card_id,
                crate::cards::CardKind::MinorImprovement,
                0,
            )?;
            if action.space == Space::MeetingPlace {
                add_accumulation(obs, &mut p, action.space);
            }
        }
        (Space::DayLaborer, Detail::None) => p.supply.food += 2,
        (Space::GrainSeeds, Detail::None) => p.supply.grain += 1,
        (Space::VegetableSeeds, Detail::None) => p.supply.vegetable += 1,
        (Space::ResourceMarket, Detail::None) => {
            p.supply.stone += 1;
            if obs.public.players.len() == 4 {
                p.supply.reed += 1;
            }
            p.supply.food += 1;
        }
        (Space::ResourceMarket, Detail::MarketChoice(good)) if obs.public.players.len() == 3 => {
            if !matches!(good, Good::Reed | Good::Stone) {
                return None;
            }
            p.supply.gain(*good, 1);
            p.supply.food += 1;
        }
        (
            Space::FamilyGrowthWithRoom | Space::ModestFamilyGrowth | Space::FamilyGrowthNoRoom,
            Detail::None,
        ) => {
            p.family += 1;
            p.newborns += 1;
        }
        (
            Space::FamilyGrowthWithRoom | Space::ModestFamilyGrowth,
            Detail::GrowWithMinor { card_id },
        ) => {
            p.family += 1;
            p.newborns += 1;
            project_play_card(
                obs,
                &mut p,
                card_id,
                crate::cards::CardKind::MinorImprovement,
                0,
            )?;
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
            let normal = p.room_material_cost.unwrap_or(5);
            let room_cost = match p.house {
                House::Wood => normal.min(p.wooden_room_cost.unwrap_or(5)),
                House::Stone => normal.saturating_sub(p.stone_cost_discount),
                House::Clay => normal,
            };
            for &cell in rooms {
                p.farm[cell].tile = Tile::Room;
                let n = p.supply.get(room_good);
                p.supply.set(room_good, n.saturating_sub(room_cost));
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
                p.improvements.iter().position(|card| {
                    matches!(card, Improvement::Fireplace2 | Improvement::Fireplace3)
                })
            } else {
                None
            };
            if let Some(index) = exchange {
                p.improvements.remove(index);
            } else {
                for (good, n) in improvement.cost() {
                    let cost = match good {
                        Good::Wood => n.saturating_sub(p.improvement_wood_discount),
                        Good::Stone => {
                            let room_discount = if p.major_stone_discount_by_rooms {
                                p.rooms().saturating_sub(2) as u32
                            } else {
                                0
                            };
                            n.saturating_sub(p.stone_cost_discount.saturating_add(room_discount))
                        }
                        _ => n,
                    };
                    let stock = p.supply.get(good);
                    p.supply.set(good, stock.saturating_sub(cost));
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
            let discount = if good == Good::Stone {
                p.stone_cost_discount
            } else {
                0
            };
            let cost = (p.rooms() as u32).saturating_sub(discount);
            p.supply.set(good, p.supply.get(good).saturating_sub(cost));
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
                let cooked_food = plan.cook * animal_cooking_rate(&p, plan.animal)?;
                p.supply.food += cooked_food;
            }
        }
        _ => return None,
    }
    let set = crate::cards::CardSet {
        definitions: obs.card_definitions.clone(),
    };
    crate::cards::after_worker(&set, &mut p, action.space).ok()?;
    // The engine applies conditional worker rewards before occupying this
    // action space, so the trigger sees only *previously* used spaces.
    crate::cards::after_worker_if_used(&set, &mut p, action.space, &obs.public.used).ok()?;
    Some(p)
}

/// Estimated change in the public-state evaluation after taking a legal space.
/// The projection is advisory; legality and consequences come from Game::apply.
pub fn action_value_with_details(
    obs: &Observation,
    space: Space,
    details: &dyn DetailPolicy,
) -> Option<f64> {
    action_value_with_details_variant(obs, space, details, EvaluationVariant::Baseline)
}

/// Same as [`action_value_with_details`] with an explicit feature variant.
pub fn action_value_with_details_variant(
    obs: &Observation,
    space: Space,
    details: &dyn DetailPolicy,
    variant: EvaluationVariant,
) -> Option<f64> {
    let current = obs.public.players.get(obs.player_id)?;
    let before = evaluate_player_with(current, obs.public.round, variant).total();
    let card_bonus_before = crate::cards::end_game_bonus_for_definitions(
        &obs.card_definitions,
        obs.public.players.get(obs.player_id)?,
    );
    action_value_with_baseline(obs, space, details, before, card_bonus_before, variant)
}

/// The per-action value with the state-only terms already evaluated.
///
/// `before` and `card_bonus_before` depend on the observation, not on the
/// candidate action, so the selection loop computes them once. Nothing after
/// this point may read hidden state: only the observation and the produced
/// action are used.
fn action_value_with_baseline(
    obs: &Observation,
    space: Space,
    details: &dyn DetailPolicy,
    before: f64,
    card_bonus_before: i32,
    variant: EvaluationVariant,
) -> Option<f64> {
    let action = details.produce(obs, space)?;
    let after = project_action(obs, &action)?;
    let mut value = evaluate_player_with(&after, obs.public.round, variant).total() - before;
    // Card end-game bonuses are intentionally not included in score(Player)
    // because their value depends on the public, externally defined card set.
    // They contribute 3 utility units per point, just like score(Player).
    // The definitions are borrowed from the observation: rebuilding a `CardSet`
    // here would clone every definition once per candidate action.
    let card_bonus_delta =
        crate::cards::end_game_bonus_for_definitions(&obs.card_definitions, &after)
            - card_bonus_before;
    value += f64::from(card_bonus_delta) * 3.0;
    if space == Space::MeetingPlace && obs.public.start_player != obs.player_id {
        value += if obs.public.round < 10 { 4.0 } else { 1.0 };
    }
    if space == Space::Fencing && obs.public.round <= 12 {
        value += 2.0;
    }
    Some(value)
}

/// Standard M2 evaluation with the same default detail policy used by the CLI.
/// For a different detail policy, use `action_value_with_details`.
pub fn action_value(obs: &Observation, space: Space) -> Option<f64> {
    action_value_with_details(obs, space, &SimpleDetails)
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

    /// #106 candidate: the variant must actually change the seed choice, and the
    /// baseline must keep its original ordering.
    #[test]
    fn vegetable_first_variant_flips_the_seed_priority() {
        let obs = observed(|_| {});
        let base_grain = action_value_with_details_variant(
            &obs,
            Space::GrainSeeds,
            &SimpleDetails,
            EvaluationVariant::Baseline,
        )
        .unwrap();
        let base_veg = action_value_with_details_variant(
            &obs,
            Space::VegetableSeeds,
            &SimpleDetails,
            EvaluationVariant::Baseline,
        )
        .unwrap();
        let veg_grain = action_value_with_details_variant(
            &obs,
            Space::GrainSeeds,
            &SimpleDetails,
            EvaluationVariant::VegetableFirst,
        )
        .unwrap();
        let veg_veg = action_value_with_details_variant(
            &obs,
            Space::VegetableSeeds,
            &SimpleDetails,
            EvaluationVariant::VegetableFirst,
        )
        .unwrap();
        assert!(
            base_grain > base_veg,
            "baseline must price grain above vegetables"
        );
        // The variant lifts the first vegetable to the first grain value. The
        // remaining tie is broken by the tactical prior (vegetables 25 vs grain
        // 23), which is what makes VegetableSeeds the preferred space.
        assert!((veg_veg - veg_grain).abs() < 1e-9);
        assert!(veg_veg > base_veg);
        // The variant must not move the grain price; only the vegetable term.
        assert!((veg_grain - base_grain).abs() < 1e-9);
    }

    /// #106 candidate 1: a one-food buffer must make a tight position less safe.
    #[test]
    fn food_buffer_variant_lowers_food_safety_under_shortage() {
        let obs = observed(|p| {
            p.family = 3;
            p.supply.food = 0;
        });
        let player = obs.public.players[obs.player_id].clone();
        let baseline = evaluate_player_with(&player, 10, EvaluationVariant::Baseline);
        let buffered = evaluate_player_with(&player, 10, EvaluationVariant::FoodBuffer);
        assert!(baseline.food_safety > buffered.food_safety);
        // A player with plenty of food must not be penalised relatively more for
        // the buffer than for the shortage; the buffer only shifts the threshold.
        let safe = observed(|p| {
            p.family = 2;
            p.supply.food = 20;
        });
        let safe_player = safe.public.players[safe.player_id].clone();
        let safe_baseline = evaluate_player_with(&safe_player, 2, EvaluationVariant::Baseline);
        let safe_buffered = evaluate_player_with(&safe_player, 2, EvaluationVariant::FoodBuffer);
        assert!((safe_baseline.food_safety - safe_buffered.food_safety).abs() < 1.0);
    }

    /// #106 candidate 2: the conditional variant must equal the unconditional one
    /// when a cooking improvement exists, and equal the baseline when food is
    /// tight and nothing can cook.
    #[test]
    fn conditional_vegetable_variant_depends_on_the_state() {
        let can_cook = observed(|p| {
            p.improvements.push(Improvement::Fireplace2);
        });
        let cook_player = can_cook.public.players[can_cook.player_id].clone();
        let cond = evaluate_player_with(&cook_player, 5, EvaluationVariant::VegetableConditional);
        let unconditional =
            evaluate_player_with(&cook_player, 5, EvaluationVariant::VegetableFirst);
        assert!((cond.resources - unconditional.resources).abs() < 1e-9);

        let tight = observed(|p| {
            p.family = 2;
            p.supply.food = 0;
        });
        let tight_player = tight.public.players[tight.player_id].clone();
        let tight_baseline = evaluate_player_with(&tight_player, 10, EvaluationVariant::Baseline);
        let tight_cond =
            evaluate_player_with(&tight_player, 10, EvaluationVariant::VegetableConditional);
        assert!((tight_baseline.resources - tight_cond.resources).abs() < 1e-9);
    }

    /// #106 candidate 3: an empty room and a growable family must be worth more.
    #[test]
    fn growth_room_variant_values_rooms_and_family_higher() {
        let obs = observed(|p| {
            p.family = 2;
            // A third room gives the family one unused room to grow into.
            if let Some(cell) = p
                .farm
                .iter_mut()
                .find(|cell| matches!(cell.tile, Tile::Empty))
            {
                cell.tile = Tile::Room;
            }
        });
        let player = obs.public.players[obs.player_id].clone();
        let baseline = evaluate_player_with(&player, 3, EvaluationVariant::Baseline);
        let variant = evaluate_player_with(&player, 3, EvaluationVariant::GrowthRoomPriority);
        assert!(variant.housing > baseline.housing);
        assert!(variant.family_growth >= baseline.family_growth);
    }

    #[test]
    fn the_hoisted_baseline_keeps_the_per_action_ranking() {
        // `select_with_details` now evaluates the state-only baseline once. Its
        // choice must still be one of the spaces with the maximum per-action
        // value, so a regression here means the hoist changed the ranking.
        for seed in 0..6u64 {
            let game = Game::new(Config {
                players: 2,
                seed,
                additional_action_space_enabled: false,
            });
            let obs = game.observation(game.public.active_player).unwrap();
            let legal = game.legal_spaces();
            let mut rng = <rand_chacha::ChaCha8Rng as rand::SeedableRng>::seed_from_u64(seed);
            let chosen = EvaluationPolicy
                .select(&obs, &legal, &mut rng)
                .expect("a legal choice");
            let values = legal
                .iter()
                .filter_map(|&space| action_value(&obs, space).map(|value| (space, value)))
                .collect::<Vec<_>>();
            let best = values
                .iter()
                .map(|(_, value)| *value)
                .fold(f64::NEG_INFINITY, f64::max);
            let chosen_value = values
                .iter()
                .find(|(space, _)| *space == chosen)
                .map(|(_, value)| *value)
                .expect("the chosen space has a value");
            assert!(
                best - chosen_value <= 1e-9,
                "seed {seed}: chosen {chosen:?} is not a maximum"
            );
        }
    }

    #[test]
    fn projecting_a_known_occupation_uses_only_own_hand() {
        use crate::cards::{CardDefinition, CardEffect, CardKind};

        let mut obs = observed(|_| {});
        obs.card_definitions.push(CardDefinition {
            use_limit: None,
            id: "sample_occ".into(),
            kind: CardKind::Occupation,
            cost: vec![],
            points: 1,
            effects: vec![CardEffect::OnPlayGain {
                good: Good::Wood,
                amount: 2,
            }],
        });
        obs.hand.push("sample_occ".into());
        let original = &obs.public.players[obs.player_id];
        let next = project_action(
            &obs,
            &Action {
                space: Space::Lessons,
                detail: Detail::PlayOccupation {
                    card_id: "sample_occ".into(),
                },
            },
        )
        .unwrap();
        assert_eq!(next.supply.wood, original.supply.wood + 2);
        assert_eq!(next.card_points, original.card_points + 1);
        assert!(next.played_cards.contains(&"sample_occ".to_owned()));
        assert!(project_action(
            &obs,
            &Action {
                space: Space::Lessons,
                detail: Detail::PlayOccupation {
                    card_id: "unknown".into(),
                },
            },
        )
        .is_none());
    }

    #[test]
    fn card_room_discount_matches_wooden_build_cost() {
        let obs = observed(|p| {
            p.supply.wood = 10;
            p.supply.reed = 2;
            p.wooden_room_cost = Some(2);
        });
        let action = SimpleDetails.produce(&obs, Space::FarmExpansion).unwrap();
        let after = project_action(&obs, &action).unwrap();
        assert_eq!(after.supply.wood, 8);
        assert_eq!(after.supply.reed, 0);
        assert_eq!(after.rooms(), obs.public.players[obs.player_id].rooms() + 1);
    }

    #[test]
    fn fencing_projection_applies_free_wood_per_action() {
        let obs = observed(|p| {
            p.supply.wood = 5;
            p.free_fence_wood = 3;
        });
        let action = SimpleDetails.produce(&obs, Space::Fencing).unwrap();
        let before = obs.public.players[obs.player_id].supply.wood;
        let after = project_action(&obs, &action).unwrap();
        assert_eq!(before - after.supply.wood, 1);
        assert_eq!(after.pastures.len(), 1);
    }

    #[test]
    fn card_end_game_bonus_affects_occupation_action_value() {
        use crate::cards::{CardDefinition, CardEffect, CardKind, ScoreBonusRule};
        let mut obs = observed(|_| {});
        obs.card_definitions.push(CardDefinition {
            use_limit: None,
            id: "bonus".into(),
            kind: CardKind::Occupation,
            cost: vec![],
            points: 0,
            effects: vec![CardEffect::EndGameBonus {
                rule: ScoreBonusRule::HouseMaterial,
            }],
        });
        obs.hand.push("bonus".into());
        let value = action_value_with_details(&obs, Space::Lessons, &SimpleDetails).unwrap();
        // A wooden home gives three end-game points; normal card score remains 0.
        assert_eq!(value, 9.0);
    }

    #[test]
    fn projection_matches_conditional_worker_card_trigger_timing() {
        use crate::cards::{CardDefinition, CardEffect, CardKind};
        let mut obs = observed(|p| p.played_cards.push("conditional".into()));
        obs.card_definitions.push(CardDefinition {
            use_limit: None,
            id: "conditional".into(),
            kind: CardKind::Occupation,
            cost: vec![],
            points: 0,
            effects: vec![CardEffect::AfterWorkerGainIfUsed {
                space: Space::GrainSeeds,
                required_space: Space::Farmland,
                good: Good::Food,
                amount: 2,
            }],
        });
        let action = Action {
            space: Space::GrainSeeds,
            detail: Detail::None,
        };
        let original = obs.public.players[obs.player_id].supply.food;
        let before = project_action(&obs, &action).unwrap();
        assert_eq!(before.supply.food, original);
        obs.public.used.insert(Space::Farmland);
        let after = project_action(&obs, &action).unwrap();
        assert_eq!(after.supply.food, original + 2);
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
            assert_eq!(
                next.supply.food,
                initial + 2 * rate,
                "{animal:?}/{improvement:?}"
            );
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
        let early_gain = evaluate_player(&improved, 3).total() - evaluate_player(&base, 3).total();
        let late_gain = evaluate_player(&improved, 14).total() - evaluate_player(&base, 14).total();
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

impl WeightedEvaluationPolicy {
    /// Use the same detail policy for estimating and executing each action.
    pub fn select_with_details(
        &self,
        obs: &Observation,
        legal: &[Space],
        rng: &mut ChaCha8Rng,
        details: &dyn DetailPolicy,
    ) -> Option<Space> {
        self.select_with_variant(obs, legal, rng, details, EvaluationVariant::Baseline)
    }

    /// Same selection with an explicit feature variant.
    pub fn select_with_variant(
        &self,
        obs: &Observation,
        legal: &[Space],
        rng: &mut ChaCha8Rng,
        details: &dyn DetailPolicy,
        variant: EvaluationVariant,
    ) -> Option<Space> {
        let mut best = f64::NEG_INFINITY;
        let mut candidates = Vec::new();
        // State-only terms do not depend on the candidate action, so they are
        // evaluated once per decision instead of once per legal space.
        let before = evaluate_player_with(
            obs.public.players.get(obs.player_id)?,
            obs.public.round,
            variant,
        )
        .total();
        let card_bonus_before = crate::cards::end_game_bonus_for_definitions(
            &obs.card_definitions,
            obs.public.players.get(obs.player_id)?,
        );
        for &space in legal {
            let Some(state_delta) =
                action_value_with_baseline(obs, space, details, before, card_bonus_before, variant)
            else {
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

/// A `#106` candidate: the same blend as [`EvaluationPolicy`] (prior 0.5, state
/// 2.0) with one [`EvaluationVariant`]. Keeping candidates as explicit variants
/// means the baseline stays byte-for-byte identical and a paired evaluation can
/// attribute any difference to the single feature that changed.
pub struct VariantEvaluationPolicy {
    pub variant: EvaluationVariant,
}

impl SpacePolicy for VariantEvaluationPolicy {
    fn select(&self, obs: &Observation, legal: &[Space], rng: &mut ChaCha8Rng) -> Option<Space> {
        WeightedEvaluationPolicy {
            prior_weight: 0.5,
            state_weight: 2.0,
        }
        .select_with_variant(obs, legal, rng, &SimpleDetails, self.variant)
    }
}

impl SpacePolicy for WeightedEvaluationPolicy {
    fn select(&self, obs: &Observation, legal: &[Space], rng: &mut ChaCha8Rng) -> Option<Space> {
        self.select_with_details(obs, legal, rng, &SimpleDetails)
    }
}

/// A policy for M3 or custom rollouts, coupled to its actual detail generator.
pub struct DetailAwareEvaluationPolicy<'a> {
    pub weights: WeightedEvaluationPolicy,
    pub details: &'a dyn DetailPolicy,
}

impl SpacePolicy for DetailAwareEvaluationPolicy<'_> {
    fn select(&self, obs: &Observation, legal: &[Space], rng: &mut ChaCha8Rng) -> Option<Space> {
        self.weights
            .select_with_details(obs, legal, rng, self.details)
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
