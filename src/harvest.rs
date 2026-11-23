//! Harvest operations, with external decisions specified in a replayable plan.
//! Official order: field phase, feeding phase, breeding phase.
use crate::breeding::BreedingPlan;
use crate::cards::{CardDefinition, CardEffect};
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
    /// C066 永遠のライ麦畑: at grain >= 3 the owner may take 1 grain INSTEAD
    /// of the 1 food. `true` opts back into the food. Default `false`
    /// (= take the grain), matching spec test C066-three and the card text
    /// 「3以上ある場合は代わりに小麦1を得る」. PR #385 review finding (b).
    #[serde(default)]
    pub c066_take_food_instead: bool,
    /// A118 樹木庭師: how many times to run the printed "pay 1 food, gain
    /// 1 wood" exchange during the harvest field phase. Capped at 2 by
    /// validation (「1つにつき食料1で購入…木材2まで」). Executed plan-driven by
    /// `harvest_fields`; the dispatcher scan stays dormant. Default 0.
    /// PR #385 review finding (c).
    #[serde(default)]
    pub a118_wood_buys: u32,
    /// A062 樽詰ビール (Beer Keg): during the feeding phase of each harvest
    /// the owner may exchange `a062_grain` grain (1..=3) for exactly 3 food
    /// and `grain - 1` bonus points. 0 declines. Validated: <= 3.
    #[serde(default)]
    pub a062_grain: u32,
    /// C055 アトリエ (Studio): during the feeding phase of each harvest the
    /// owner may run exactly one conversion: wood/clay -> 2 food, stone ->
    /// 3 food. None declines.
    #[serde(default)]
    pub c055_convert: Option<Good>,
    /// C059 シュナップス蒸留所 (Schnapps Distillery): during the feeding
    /// phase of each harvest the owner may cook 1 vegetable into 5 food.
    #[serde(default)]
    pub c059_cook: bool,
    /// C063 クラフトビール醸造所 (Craft Brewery): during the feeding phase
    /// of each harvest the owner may brew: 1 grain from supply AND 1 grain
    /// taken from a standing grain field (taken by `c063_field_taken` at the
    /// start of the field phase) -> 4 food and 2 points.
    #[serde(default)]
    pub c063_brew: bool,
    /// D062 ビールタップ (Beer Tap): during the feeding phase of each
    /// harvest the owner may exchange exactly 2/3/4 grain for 3/6/9 food
    /// (one shared choice per harvest). 0 declines. Validated: 0/2/3/4.
    #[serde(default)]
    pub d062_grain: u32,
    /// D084 飼料ペレット (Feed Pellets): during the feeding phase of each
    /// harvest the owner may convert 1 vegetable into 1 animal of a type
    /// already owned (placed into an existing pasture/stable with room).
    /// None declines.
    #[serde(default)]
    pub d084_animal: Option<Animal>,
}
impl Default for FeedingPlan {
    fn default() -> Self {
        Self {
            priorities: FeedSource::ALL.to_vec(),
            preserve_pairs_first: true,
            extra_crafting: vec![],
            c066_take_food_instead: false,
            a118_wood_buys: 0,
            a062_grain: 0,
            c055_convert: None,
            c059_cook: false,
            c063_brew: false,
            d062_grain: 0,
            d084_animal: None,
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
        if self.a118_wood_buys > A118_MAX_WOOD_BUYS {
            return Err("a118 wood buys exceed the printed cap of 2".into());
        }
        if self.a062_grain > 3 {
            return Err("a062 grain exchange must be 0..=3".into());
        }
        if !matches!(self.d062_grain, 0 | 2 | 3 | 4) {
            return Err("d062 grain exchange must be 0, 2, 3 or 4".into());
        }
        if let Some(good) = self.c055_convert {
            if !matches!(good, Good::Wood | Good::Clay | Good::Stone) {
                return Err("c055 conversion must be wood, clay or stone".into());
            }
        }
        Ok(())
    }
}

/// A118 樹木庭師 printed exchange cap: 「追加で木材2まで」.
pub const A118_MAX_WOOD_BUYS: u32 = 2;

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
    pub(crate) fn harvest_fields(&mut self) -> Result<()> {
        // Cardless games have no harvest effects at all, so skip the dispatch
        // entirely: this is the hot path for the cardless baseline.
        let carded = self.card_set.enabled();
        // The field phase is one effect chain across all players, so the budget
        // is reset once here rather than per player.
        if carded {
            self.event_budget = crate::model::DEFAULT_EVENT_BUDGET;
        }
        // Capture the D032 snapshot (goods standing in fields) at the start of
        // the final-harvest field phase, BEFORE any field's `remaining` is
        // decremented. The spec counts goods that stood before the final
        // harvest, not what remains after each field yields one good.
        if self.public.round == 14 {
            for player in &mut self.public.players {
                let goods: u32 = player
                    .farm
                    .iter()
                    .map(|cell| match cell.tile {
                        Tile::Field {
                            crop: Some(_),
                            remaining,
                        } => remaining as u32,
                        _ => 0,
                    })
                    .sum();
                player.field_goods_before_final_harvest = Some(goods);
            }
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
                    &mut self.public.card_uses,
                    &self.card_use_limits,
                    &mut self.pending,
                )?;
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
        Ok(())
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
        // C063 クラフトビール醸造所: the brewery takes 1 grain from a
        // standing grain field at the start of the field phase, before the
        // fields yield. Recorded per player so the feeding step knows the
        // field grain was secured (the supply half is settled there).
        let mut c063_field_taken = vec![false; self.public.players.len()];
        if self.card_set.enabled() {
            for (id, plan) in feeding.iter().enumerate() {
                if !plan.c063_brew {
                    continue;
                }
                let owns_c063 = self.public.players[id]
                    .played_cards
                    .iter()
                    .any(|c| c == "C063");
                if !owns_c063 {
                    continue;
                }
                let mut found = None;
                for cell in &mut self.public.players[id].farm {
                    if let Tile::Field {
                        crop: Some(Crop::Grain),
                        mut remaining,
                    } = cell.tile
                    {
                        if remaining > 0 {
                            remaining -= 1;
                            cell.tile = if remaining == 0 {
                                Tile::Field {
                                    crop: None,
                                    remaining: 0,
                                }
                            } else {
                                Tile::Field {
                                    crop: Some(Crop::Grain),
                                    remaining,
                                }
                            };
                            found = Some(());
                            break;
                        }
                    }
                }
                let Some(()) = found else { continue };
                c063_field_taken[id] = true;
            }
        }
        self.harvest_fields()?;
        // A118 樹木庭師: plan-driven optional exchanges, run after the field
        // dispatcher granted the printed fixed wood. The plan count is
        // validated (<= 2) and bounded by the food actually available.
        for (id, plan) in feeding.iter().enumerate() {
            for _ in 0..plan.a118_wood_buys {
                let p = &mut self.public.players[id];
                let owns_a118 = p.played_cards.iter().any(|c| c == "A118");
                if !owns_a118 {
                    return Err("a118 wood buys require the printed card".into());
                }
                if p.supply.food < 1 {
                    return Err("a118 wood buys exceed the available food".into());
                }
                p.supply.food -= 1;
                p.supply.gain(Good::Wood, 1);
            }
        }
        // Feeding-phase exchanges: automatic gains (D012, A148) and the owner-chosen
        // exchanges (A062, C055, C059, C063, D062, D084). Runs before
        // feeding so converted food can cover the feeding cost. Every choice
        // is plan-driven and best-effort: an unaffordable or not-owned choice
        // resolves to nothing (spec tests A062-not-enough / D062-not-enough).
        execute_harvest_feeding_exchanges(
            &mut self.public.players,
            feeding,
            &c063_field_taken,
            &self.card_set.definitions,
        )?;
        self.feed_after_fields(feeding)?;
        for (player, plan) in self.public.players.iter_mut().zip(breeding) {
            plan.apply(player)?;
        }
        // Field phase -> feeding -> breeding are done: fire the "end of each
        // harvest" cards (A145 reed, C066 rye bonus). New tail dispatch added
        // by the PR #385 review-fix round (review finding (a)).
        if self.card_set.enabled() {
            self.event_budget = crate::model::DEFAULT_EVENT_BUDGET;
            for p in 0..self.public.players.len() {
                let take_food_instead = feeding
                    .get(p)
                    .map(|plan| plan.c066_take_food_instead)
                    .unwrap_or(false);
                crate::card_events::dispatch_after_harvest_complete(
                    &self.card_set,
                    p,
                    &mut self.cards,
                    &mut self.public.players,
                    &mut self.public.improvement_market,
                    &mut self.delayed_well,
                    self.public.round,
                    &self.public.used,
                    &mut self.event_budget,
                    &mut self.public.card_uses,
                    &self.card_use_limits,
                    &mut self.pending,
                    take_food_instead,
                )?;
            }
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

/// Resolve the feeding-phase effects granted during this harvest: automatic
/// gains (D012, A148) and the owner-chosen exchanges (A062, C055, C059, C063,
/// D062, D084). Called from [`Game::harvest`] before [`feed`] so converted
/// food can cover the feeding cost.
///
/// Every choice is plan-driven and best-effort: a choice for a card the
/// player does not own, or that the player cannot afford, resolves to
/// nothing (spec tests A062-not-enough / D062-not-enough). `c063_field_taken`
/// is produced by the start-of-field-phase pass in [`Game::harvest`], which
/// secures 1 grain from a standing grain field before the fields yield.
fn execute_harvest_feeding_exchanges(
    players: &mut [Player],
    plans: &[FeedingPlan],
    c063_field_taken: &[bool],
    card_definitions: &[CardDefinition],
) -> Result<()> {
    // Build a map from card_id to its effects for quick lookup.
    let card_effects: std::collections::HashMap<&str, &Vec<CardEffect>> = card_definitions
        .iter()
        .map(|c| (c.id.as_str(), &c.effects))
        .collect();

    for (id, (player, plan)) in players.iter_mut().zip(plans.iter()).enumerate() {
        let owns =
            |card_id: &str, played_cards: &[String]| played_cards.iter().any(|c| c == card_id);
        // Collect automatic harvest feeding gains (D012 food, A148 sheep) first
        // to avoid borrowing player while mutating it.
        let mut auto_gains = Vec::new();
        for cid in &player.played_cards {
            if let Some(effects) = card_effects.get(cid.as_str()) {
                for effect in effects.iter() {
                    if let CardEffect::HarvestFeedingGain {
                        good,
                        amount,
                        animal,
                    } = effect
                    {
                        auto_gains.push((*good, *amount, *animal));
                    }
                }
            }
        }
        // Apply collected automatic gains
        for (good, amount, animal) in auto_gains {
            if let Some(good) = good {
                player.supply.gain(good, amount);
            }
            if let Some(animal) = animal {
                if player.supply.food > 0 || true {
                    let _ = place_one_animal(player, animal);
                }
            }
        }

        // A062 樽詰ビール (Beer Keg): exchange n grain (1..=3) for exactly
        // 3 food and n - 1 bonus points.
        if plan.a062_grain > 0
            && owns("A062", &player.played_cards)
            && player.supply.grain >= plan.a062_grain
        {
            let n = plan.a062_grain;
            player.supply.grain -= n;
            player.supply.food = player.supply.food.checked_add(3).ok_or("food overflow")?;
            player.card_points = player
                .card_points
                .checked_add(i32::try_from(n - 1).map_err(|_| "points overflow")?)
                .ok_or("points overflow")?;
        }

        // C055 アトリエ (Studio): exactly one of wood/clay -> 2 food or
        // stone -> 3 food.
        if let Some(good) = plan.c055_convert {
            if owns("C055", &player.played_cards) && player.supply.get(good) >= 1 {
                player.supply.pay(good, 1)?;
                let food = match good {
                    Good::Wood | Good::Clay => 2,
                    Good::Stone => 3,
                    _ => unreachable!("c055_convert is validated to wood/clay/stone"),
                };
                player.supply.food = player
                    .supply
                    .food
                    .checked_add(food)
                    .ok_or("food overflow")?;
            }
        }

        // C059 シュナップス蒸留所 (Schnapps Distillery): cook 1 vegetable
        // into 5 food.
        if plan.c059_cook && owns("C059", &player.played_cards) && player.supply.vegetable >= 1 {
            player.supply.vegetable -= 1;
            player.supply.food = player.supply.food.checked_add(5).ok_or("food overflow")?;
        }

        // C063 クラフトビール醸造所 (Craft Brewery): 1 grain from supply
        // plus the field grain secured at the start of the field phase ->
        // 4 food and 2 bonus points.
        if plan.c063_brew
            && owns("C063", &player.played_cards)
            && c063_field_taken[id]
            && player.supply.grain >= 1
        {
            player.supply.grain -= 1;
            player.supply.food = player.supply.food.checked_add(4).ok_or("food overflow")?;
            player.card_points = player.card_points.checked_add(2).ok_or("points overflow")?;
        }

        // D062 ビールタップ (Beer Tap): exchange exactly 2/3/4 grain for
        // 3/6/9 food (one shared choice per harvest).
        if plan.d062_grain >= 2
            && owns("D062", &player.played_cards)
            && player.supply.grain >= plan.d062_grain
        {
            let n = plan.d062_grain;
            player.supply.grain -= n;
            let food = match n {
                2 => 3,
                3 => 6,
                4 => 9,
                _ => unreachable!("d062_grain is validated to 0/2/3/4"),
            };
            player.supply.food = player
                .supply
                .food
                .checked_add(food)
                .ok_or("food overflow")?;
        }

        // D084 飼料ペレット (Feed Pellets): convert 1 vegetable into 1
        // animal of a type already owned, placed into an existing pasture
        // or stable with room.
        if let Some(animal) = plan.d084_animal {
            if owns("D084", &player.played_cards)
                && player.supply.vegetable >= 1
                && place_one_animal(player, animal)
            {
                player.supply.vegetable -= 1;
            }
        }
    }
    Ok(())
}

/// Place one `animal` into the first pasture holding this species (or an
/// empty pasture) with free capacity, else into the first empty unfenced
/// stable cell. Returns false when there is no room: the calling exchange
/// then resolves to nothing (no animal is taken).
fn place_one_animal(player: &mut Player, animal: Animal) -> bool {
    let capacities: Vec<u32> = player
        .pastures
        .iter()
        .map(|past| player.pasture_capacity(past))
        .collect();
    for (i, past) in player.pastures.iter_mut().enumerate() {
        if past.quantity < capacities[i] && (past.animal == Some(animal) || past.animal.is_none()) {
            past.animal = Some(animal);
            past.quantity += 1;
            return true;
        }
    }
    if let Some(i) = player
        .farm
        .iter()
        .position(|c| c.stable && matches!(c.tile, Tile::Empty) && c.stable_animal.is_none())
    {
        player.farm[i].stable_animal = Some(animal);
        return true;
    }
    false
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
