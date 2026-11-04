//! Harvest operations, with external decisions specified in a replayable plan.
//! Official order: field phase, feeding phase, breeding phase.
use crate::engine::Result;
use crate::model::*;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FeedSource {
    StoredFood,
    Joinery,
    Pottery,
    Basketry,
    Vegetables,
    Grain,
    Sheep,
    Boar,
    Cattle,
}

impl FeedSource {
    pub const ALL: [Self; 9] = [
        Self::StoredFood,
        Self::Joinery,
        Self::Pottery,
        Self::Basketry,
        Self::Vegetables,
        Self::Grain,
        Self::Sheep,
        Self::Boar,
        Self::Cattle,
    ];
}

/// A decision artifact from the policy: which resources to use first and
/// whether to defer eating livestock breeding pairs until other resources
/// have been considered. The rule engine validates and executes the plan.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FeedingPlan {
    pub priorities: Vec<FeedSource>,
    pub preserve_pairs_first: bool,
}
impl Default for FeedingPlan {
    fn default() -> Self {
        Self {
            priorities: FeedSource::ALL.to_vec(),
            preserve_pairs_first: true,
        }
    }
}
impl FeedingPlan {
    pub fn validate(&self) -> Result<()> {
        let distinct: BTreeSet<_> = self.priorities.iter().copied().collect();
        if self.priorities.len() != FeedSource::ALL.len()
            || distinct != FeedSource::ALL.into_iter().collect()
        {
            return Err("feeding order must contain each source once".into());
        }
        Ok(())
    }
}

/// The policy sees only a public observation after field crops are harvested;
/// the engine logs all resulting plans for deterministic action-only replay.
pub trait HarvestDetailPolicy {
    fn plan_feeding(&self, obs: &Observation) -> FeedingPlan;
}
pub struct SimpleHarvestDetails;
impl HarvestDetailPolicy for SimpleHarvestDetails {
    fn plan_feeding(&self, _: &Observation) -> FeedingPlan {
        FeedingPlan::default()
    }
}

impl Game {
    /// Only an internal preview copy is used to query policies at the
    /// beginning of the harvest. The real field phase happens in harvest().
    pub(crate) fn harvest_fields(&mut self) {
        for p in &mut self.public.players {
            for cell in &mut p.farm {
                if let Tile::Field {
                    crop: Some(kind),
                    remaining,
                } = &mut cell.tile
                {
                    if *remaining > 0 {
                        let good = if *kind == Crop::Grain {
                            Good::Grain
                        } else {
                            Good::Vegetable
                        };
                        p.supply.gain(good, 1);
                        *remaining -= 1;
                        if *remaining == 0 {
                            cell.tile = Tile::Field {
                                crop: None,
                                remaining: 0,
                            };
                        }
                    }
                }
            }
        }
    }
    pub(crate) fn harvest(&mut self, plans: &[FeedingPlan]) -> Result<()> {
        if !HARVEST_ROUNDS.contains(&self.public.round) {
            return Err("harvest in non-harvest round".into());
        }
        if plans.len() != self.public.players.len() {
            return Err("feeding plan per player required".into());
        }
        for plan in plans {
            plan.validate()?;
        }
        self.harvest_fields();
        for (p, plan) in self.public.players.iter_mut().zip(plans) {
            feed(p, plan)?;
            breed(p)?;
        }
        Ok(())
    }
}
fn use_food(p: &mut Player, owed: &mut u32, created: u32) {
    let spent = created.min(*owed);
    *owed -= spent;
    p.supply.food += created - spent;
}
fn animal_food(p: &Player, animal: Animal) -> Option<u32> {
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
fn feed_animals(p: &mut Player, owed: &mut u32, species: Animal, preserve_pair: bool) {
    if *owed == 0 {
        return;
    }
    let food = match animal_food(p, species) {
        Some(v) => v,
        None => return,
    };
    let mut removable = p
        .animal_count(species)
        .saturating_sub(if preserve_pair { 2 } else { 0 });
    for pasture in &mut p.pastures {
        if *owed == 0 || removable == 0 {
            break;
        }
        if pasture.animal == Some(species) {
            while *owed > 0 && removable > 0 && pasture.quantity > 0 {
                pasture.quantity -= 1;
                removable -= 1;
                let spent = (*owed).min(food);
                *owed -= spent;
                p.supply.food += food - spent;
            }
            if pasture.quantity == 0 {
                pasture.animal = None;
            }
        }
    }
    if *owed > 0 && removable > 0 && p.pet == Some(species) {
        p.pet = None;
        removable -= 1;
        use_food(p, owed, food);
    }
    for stable in &mut p.farm {
        if *owed == 0 || removable == 0 {
            break;
        }
        if stable.stable_animal == Some(species) {
            stable.stable_animal = None;
            removable -= 1;
            let spent = (*owed).min(food);
            *owed -= spent;
            p.supply.food += food - spent;
        }
    }
}
fn feed(p: &mut Player, plan: &FeedingPlan) -> Result<()> {
    plan.validate()?;
    let mut owed = (p.family - p.newborns) as u32 * 2 + p.newborns as u32;
    for source in &plan.priorities {
        if owed == 0 {
            break;
        }
        match source {
            FeedSource::StoredFood => {
                let n = owed.min(p.supply.food);
                owed -= n;
                p.supply.food -= n;
            }
            FeedSource::Joinery | FeedSource::Pottery | FeedSource::Basketry => {
                let (card, good, yield_food) = match source {
                    FeedSource::Joinery => (Improvement::Joinery, Good::Wood, 2),
                    FeedSource::Pottery => (Improvement::Pottery, Good::Clay, 2),
                    _ => (Improvement::Basketry, Good::Reed, 3),
                };
                if p.improvements.contains(&card) && p.supply.get(good) > 0 {
                    p.supply.pay(good, 1)?;
                    use_food(p, &mut owed, yield_food);
                }
            }
            FeedSource::Vegetables => {
                let value = if p
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
                while owed > 0 && p.supply.vegetable > 0 {
                    p.supply.vegetable -= 1;
                    use_food(p, &mut owed, value);
                }
            }
            FeedSource::Grain => {
                while owed > 0 && p.supply.grain > 0 {
                    p.supply.grain -= 1;
                    owed -= 1;
                }
            }
            FeedSource::Sheep => {
                feed_animals(p, &mut owed, Animal::Sheep, plan.preserve_pairs_first)
            }
            FeedSource::Boar => feed_animals(p, &mut owed, Animal::Boar, plan.preserve_pairs_first),
            FeedSource::Cattle => {
                feed_animals(p, &mut owed, Animal::Cattle, plan.preserve_pairs_first)
            }
        }
    }
    if owed > 0 && plan.preserve_pairs_first {
        // Do not take begging markers while edible livestock still exists.
        for source in &plan.priorities {
            match source {
                FeedSource::Sheep => feed_animals(p, &mut owed, Animal::Sheep, false),
                FeedSource::Boar => feed_animals(p, &mut owed, Animal::Boar, false),
                FeedSource::Cattle => feed_animals(p, &mut owed, Animal::Cattle, false),
                _ => (),
            }
        }
    }
    p.begging += owed;
    Ok(())
}
fn breed(p: &mut Player) -> Result<()> {
    for species in [Animal::Sheep, Animal::Boar, Animal::Cattle] {
        if p.animal_count(species) < 2 {
            continue;
        }
        if let Some(index) = p
            .pastures
            .iter()
            .position(|q| q.animal == Some(species) && q.quantity < p.pasture_capacity(q))
        {
            p.pastures[index].quantity += 1;
            continue;
        }
        if let Some(index) = p
            .pastures
            .iter()
            .position(|q| q.animal.is_none() && q.quantity == 0)
        {
            p.pastures[index].animal = Some(species);
            p.pastures[index].quantity = 1;
            continue;
        }
        if let Some(cell) = p.farm.iter_mut().find(|c| {
            c.stable && !matches!(c.tile, Tile::Pasture { .. }) && c.stable_animal.is_none()
        }) {
            cell.stable_animal = Some(species);
            continue;
        }
        if p.pet.is_none() {
            p.pet = Some(species);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn newborn_costs_one_food_and_is_reset_after_harvest() {
        let mut player = Player::new(0, 3);
        player.family = 3;
        player.newborns = 1;
        feed(&mut player, &FeedingPlan::default()).unwrap();
        assert_eq!(player.begging, 2);
    }
    #[test]
    fn edible_breeding_pair_must_be_used_to_feed() {
        let mut player = Player::new(0, 0);
        player.improvements.push(Improvement::Hearth4);
        player.pet = Some(Animal::Sheep);
        player.farm[14].stable = true;
        player.farm[14].stable_animal = Some(Animal::Sheep);
        feed(&mut player, &FeedingPlan::default()).unwrap();
        assert_eq!(player.begging, 0);
        assert_eq!(player.animal_count(Animal::Sheep), 0);
    }
    #[test]
    fn priorities_can_preserve_grain_when_food_is_sufficient() {
        let mut original = Player::new(0, 2);
        original.supply.grain = 4;
        original.improvements.push(Improvement::Hearth4);
        original.pet = Some(Animal::Cattle);
        let mut eat_grain = original.clone();
        let mut eat_cattle = original.clone();
        let grain_first = FeedingPlan::default();
        let mut cattle_first = FeedingPlan::default();
        cattle_first.priorities.swap(5, 8);
        cattle_first.preserve_pairs_first = false;
        feed(&mut eat_grain, &grain_first).unwrap();
        feed(&mut eat_cattle, &cattle_first).unwrap();
        assert!(eat_cattle.supply.grain > eat_grain.supply.grain);
    }
    #[test]
    fn plan_rejects_missing_and_repeated_sources() {
        let mut plan = FeedingPlan::default();
        plan.priorities.push(FeedSource::Cattle);
        assert!(plan.validate().is_err());
        plan.priorities.pop();
        plan.priorities.pop();
        assert!(plan.validate().is_err());
    }
}
