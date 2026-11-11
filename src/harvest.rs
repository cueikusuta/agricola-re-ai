//! Harvest operations, with external decisions specified in a replayable plan.
//! Official order: field phase, feeding phase, breeding phase.
use crate::breeding::BreedingPlan;
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
    /// Voluntary use of workshop processing, even if the family is already fed.
    #[serde(default)]
    pub extra_crafting: Vec<Improvement>,
}
impl Default for FeedingPlan {
    fn default() -> Self {
        Self {
            priorities: FeedSource::ALL.to_vec(),
            preserve_pairs_first: true,
            extra_crafting: vec![],
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
        let chosen: BTreeSet<_> = self.extra_crafting.iter().copied().collect();
        if chosen.len() != self.extra_crafting.len()
            || self.extra_crafting.iter().any(|card| {
                !matches!(
                    card,
                    Improvement::Joinery | Improvement::Pottery | Improvement::Basketry
                )
            })
        {
            return Err("extra crafting must list each owned workshop at most once".into());
        }
        Ok(())
    }
}

/// The policy sees only a public observation after field crops are harvested;
/// the engine logs all resulting plans for deterministic action-only replay.
pub trait HarvestDetailPolicy {
    fn plan_feeding(&self, obs: &Observation) -> FeedingPlan;
    fn plan_breeding(&self, obs: &Observation) -> BreedingPlan {
        BreedingPlan::simple(&obs.public.players[obs.player_id])
    }
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
        // Cardless games have no harvest effects at all, so skip the dispatch
        // entirely: this is the hot path for the cardless baseline.
        let carded = self.card_set.enabled();
        // The field phase is one effect chain across all players, so the budget
        // is reset once here rather than per player.
        if carded {
            self.event_budget = crate::model::DEFAULT_EVENT_BUDGET;
        }
        for p in 0..self.public.players.len() {
            // Route the harvest trigger through the dispatcher so the event
            // budget and per-card use limits apply.
            if carded {
                crate::card_events::dispatch_harvest_field(
                    &self.card_set,
                    p,
                    &mut self.cards,
                    &mut self.public.players,
                    &mut self.public.improvement_market,
                    &mut self.delayed_well,
                    self.public.round,
                    &self.public.used,
                    &mut self.event_budget,
                    &mut self.card_uses,
                    &self.card_use_limits,
                    &mut self.pending,
                )
                .expect("validated card field-phase reward");
            }
            let p = &mut self.public.players[p];
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
    /// Called only on the preview AFTER field harvesting, to generate
    /// breeding decisions from the actual post-feeding public state.
    pub(crate) fn feed_after_fields(&mut self, plans: &[FeedingPlan]) -> Result<()> {
        if plans.len() != self.public.players.len() {
            return Err("feeding plan per player required".into());
        }
        for (player, plan) in self.public.players.iter_mut().zip(plans) {
            feed(player, plan)?;
        }
        Ok(())
    }

    pub(crate) fn harvest(
        &mut self,
        feeding: &[FeedingPlan],
        breeding: &[BreedingPlan],
    ) -> Result<()> {
        if !HARVEST_ROUNDS.contains(&self.public.round) {
            return Err("harvest in non-harvest round".into());
        }
        if breeding.len() != self.public.players.len() {
            return Err("breeding plan per player required".into());
        }
        self.harvest_fields();
        self.feed_after_fields(feeding)?;
        for (player, plan) in self.public.players.iter_mut().zip(breeding) {
            plan.apply(player)?;
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
    let mut used_workshops = BTreeSet::new();
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
                    used_workshops.insert(card);
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
    // A workshop may process one resource per harvest whether or not the
    // family still needs feeding. It may not process a second time.
    for card in &plan.extra_crafting {
        if used_workshops.contains(card) || !p.improvements.contains(card) {
            return Err("craft workshop not owned or already used this harvest".into());
        }
        let (good, produced) = match card {
            Improvement::Joinery => (Good::Wood, 2),
            Improvement::Pottery => (Good::Clay, 2),
            Improvement::Basketry => (Good::Reed, 3),
            _ => return Err("invalid craft workshop".into()),
        };
        p.supply.pay(good, 1)?;
        use_food(p, &mut owed, produced);
        used_workshops.insert(*card);
    }
    p.begging += owed;
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

#[cfg(test)]
mod craft_option_tests {
    use super::*;

    #[test]
    fn craft_processing_is_optional_even_after_all_meals_paid() {
        let mut player = Player::new(0, 4);
        player.improvements.push(Improvement::Joinery);
        player.supply.wood = 2;
        let plan = FeedingPlan {
            extra_crafting: vec![Improvement::Joinery],
            ..FeedingPlan::default()
        };
        feed(&mut player, &plan).unwrap();
        assert_eq!(player.begging, 0);
        assert_eq!(player.supply.wood, 1);
        assert_eq!(player.supply.food, 2);

        let mut player = Player::new(0, 4);
        player.improvements.push(Improvement::Joinery);
        player.supply.wood = 2;
        feed(&mut player, &FeedingPlan::default()).unwrap();
        assert_eq!(player.supply.wood, 2);
        assert_eq!(player.supply.food, 0);
    }

    #[test]
    fn each_owned_craft_workshop_may_process_once_per_harvest() {
        let mut player = Player::new(0, 4);
        player.improvements.extend([
            Improvement::Joinery,
            Improvement::Pottery,
            Improvement::Basketry,
        ]);
        player.supply.wood = 1;
        player.supply.clay = 1;
        player.supply.reed = 1;
        let plan = FeedingPlan {
            extra_crafting: vec![
                Improvement::Joinery,
                Improvement::Pottery,
                Improvement::Basketry,
            ],
            ..FeedingPlan::default()
        };
        feed(&mut player, &plan).unwrap();
        assert_eq!(player.supply.food, 7);
        assert_eq!(player.supply.wood, 0);
        assert_eq!(player.supply.clay, 0);
        assert_eq!(player.supply.reed, 0);
    }

    #[test]
    fn invalid_workshop_reuse_and_unowned_conversion_are_rejected() {
        let mut player = Player::new(0, 0);
        player.improvements.push(Improvement::Joinery);
        player.supply.wood = 2;
        let plan = FeedingPlan {
            extra_crafting: vec![Improvement::Joinery],
            ..FeedingPlan::default()
        };
        assert!(feed(&mut player, &plan).is_err());

        let mut plan = FeedingPlan {
            extra_crafting: vec![Improvement::Joinery, Improvement::Joinery],
            ..FeedingPlan::default()
        };
        assert!(plan.validate().is_err());
        plan.extra_crafting = vec![Improvement::Hearth4];
        assert!(plan.validate().is_err());

        let mut player = Player::new(0, 4);
        player.supply.wood = 1;
        let plan = FeedingPlan {
            extra_crafting: vec![Improvement::Joinery],
            ..FeedingPlan::default()
        };
        assert!(feed(&mut player, &plan).is_err());
    }

    #[test]
    fn old_feeding_plan_json_defaults_extra_crafting_to_no_processing() {
        let serialized = serde_json::to_string(&FeedingPlan::default()).unwrap();
        let mut value: serde_json::Value = serde_json::from_str(&serialized).unwrap();
        value.as_object_mut().unwrap().remove("extra_crafting");
        let recovered: FeedingPlan = serde_json::from_value(value).unwrap();
        assert!(recovered.extra_crafting.is_empty());
    }
}
