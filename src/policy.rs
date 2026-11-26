//! Replaceable AI and subaction detail policies. Only Observation is accepted.
use crate::decision_budget::{DecisionBudgetSpec, DecisionOutcome};
use crate::engine::{Action, AnimalPlan, BakeAllocation, Detail, Plant};
use crate::harvest::{HarvestDetailPolicy, SimpleHarvestDetails};
use crate::model::*;
use rand::{seq::SliceRandom, SeedableRng};
use rand_chacha::ChaCha8Rng;
use std::time::Instant;

pub trait SpacePolicy {
    fn select(&self, obs: &Observation, legal: &[Space], rng: &mut ChaCha8Rng) -> Option<Space>;

    /// Deadline-aware selection (#167 A).
    ///
    /// The default ignores the deadline: heuristic policies cost microseconds,
    /// so spending budget on them would not buy anything. Search policies
    /// (MCTS/PUCT) override this to consume the search window of a
    /// [`DecisionBudget`]. `deadline = None` must keep the historical, fully
    /// reproducible behaviour.
    fn select_with_deadline(
        &self,
        obs: &Observation,
        legal: &[Space],
        rng: &mut ChaCha8Rng,
        _deadline: Option<Instant>,
    ) -> Option<Space> {
        self.select(obs, legal, rng)
    }
}
pub trait DetailPolicy {
    fn produce(&self, obs: &Observation, space: Space) -> Option<Action>;

    /// Deadline-aware detail generation (#167 A). The deadline is the *whole
    /// decision* deadline, so a policy that can check it must leave the action
    /// legal when it gives up. No implementation can be preempted mid-call, so
    /// the caller still measures and records any overshoot.
    fn produce_with_deadline(
        &self,
        obs: &Observation,
        space: Space,
        _deadline: Option<Instant>,
    ) -> Option<Action> {
        self.produce(obs, space)
    }
}
pub struct RandomPolicy;
pub struct GreedyPolicy;
pub struct SimpleDetails;
impl SpacePolicy for RandomPolicy {
    fn select(&self, _: &Observation, legal: &[Space], rng: &mut ChaCha8Rng) -> Option<Space> {
        legal.choose(rng).copied()
    }
}
pub(crate) fn weight(obs: &Observation, space: Space) -> i32 {
    let p = &obs.public.players[obs.player_id];
    let r = obs.public.round;
    match space {
        Space::Forest
        | Space::Copse
        | Space::Grove
        | Space::Hollow
        | Space::TravelingPlayers
        | Space::ClayPit
        | Space::ReedBank
        | Space::WesternQuarry
        | Space::EasternQuarry
        | Space::Fishing
        | Space::MeetingPlace => {
            let stored = obs.public.accumulations.get(&space).copied().unwrap_or(0) as i32;
            let value = match space {
                Space::Forest | Space::Copse | Space::Grove => 3,
                Space::ReedBank => 4,
                Space::WesternQuarry | Space::EasternQuarry => 4,
                Space::Fishing | Space::MeetingPlace | Space::TravelingPlayers => 5,
                _ => 3,
            };
            stored * value
                + if space == Space::MeetingPlace && obs.public.start_player != obs.player_id {
                    3
                } else {
                    0
                }
        }
        Space::DayLaborer => {
            if p.supply.food < 4 {
                15
            } else {
                7
            }
        }
        Space::FarmExpansion => {
            if p.supply.reed >= 2 && p.supply.wood >= 5 && p.family == p.rooms() as u8 {
                70
            } else {
                9
            }
        }
        Space::FamilyGrowthWithRoom | Space::ModestFamilyGrowth => 80 - 2 * r as i32,
        Space::FamilyGrowthNoRoom => 75,
        Space::Fencing => 14,
        Space::Farmland => {
            if p.supply.grain > 0 {
                19
            } else {
                12
            }
        }
        Space::GrainSeeds => {
            if p.supply.grain == 0 {
                23
            } else {
                11
            }
        }
        Space::VegetableSeeds => 25,
        Space::GrainUtilization | Space::Cultivation => 26,
        Space::MajorImprovement => {
            if p.improvements.iter().all(|i| !i.is_cooking()) {
                24
            } else {
                12
            }
        }
        Space::SheepMarket | Space::PigMarket | Space::CattleMarket => {
            obs.public
                .animal_accumulations
                .get(&space)
                .copied()
                .unwrap_or(0) as i32
                * 3
        }
        Space::AnimalMarket => 10,
        Space::ResourceMarket => 13,
        Space::HouseRedevelopment => {
            if r >= 11 {
                35
            } else {
                8
            }
        }
        Space::FarmRedevelopment => 35,
        Space::Lessons | Space::ExtraLessons => 16,
    }
}
impl SpacePolicy for GreedyPolicy {
    fn select(&self, obs: &Observation, legal: &[Space], rng: &mut ChaCha8Rng) -> Option<Space> {
        let max = legal.iter().map(|&s| weight(obs, s)).max()?;
        legal
            .iter()
            .copied()
            .filter(|&s| weight(obs, s) == max)
            .collect::<Vec<_>>()
            .choose(rng)
            .copied()
    }
}
fn empty_room_target(p: &Player) -> Option<usize> {
    (0..FARM_CELLS).find(|&i| {
        matches!(p.farm[i].tile, Tile::Empty)
            && !p.farm[i].stable
            && neighbors(i)
                .iter()
                .any(|&j| matches!(p.farm[j].tile, Tile::Room))
    })
}
/// Preserve historical preference for open ground while allowing a fenced
/// pasture when no unfenced site remains (Lookout revised rules, p. 8).
fn stable_target(p: &Player) -> Option<usize> {
    (0..FARM_CELLS)
        .find(|&i| {
            matches!(p.farm[i].tile, Tile::Empty) && crate::engine::stable_site_available(p, i)
        })
        .or_else(|| {
            (0..FARM_CELLS).find(|&i| {
                matches!(p.farm[i].tile, Tile::Pasture { .. })
                    && crate::engine::stable_site_available(p, i)
            })
        })
}
fn empty_field_target(p: &Player) -> Option<usize> {
    (0..FARM_CELLS).find(|&i| {
        matches!(p.farm[i].tile, Tile::Empty)
            && !p.farm[i].stable
            && (p.fields() == 0
                || neighbors(i)
                    .iter()
                    .any(|&j| matches!(p.farm[j].tile, Tile::Field { .. })))
    })
}
fn small_pasture(p: &Player) -> Option<Vec<usize>> {
    crate::engine::single_cell_pasture(p).map(|i| vec![i])
}
fn plantings(p: &Player, new_field: Option<usize>) -> Vec<Plant> {
    let mut gr = p.supply.grain;
    let mut ve = p.supply.vegetable;
    let mut fields = (0..FARM_CELLS)
        .filter(|&i| matches!(p.farm[i].tile, Tile::Field { crop: None, .. }))
        .collect::<Vec<_>>();
    if let Some(i) = new_field {
        fields.push(i)
    }
    let mut result = vec![];
    for cell in fields {
        if gr > 0 {
            result.push(Plant {
                cell,
                crop: Crop::Grain,
            });
            gr -= 1
        } else if ve > 0 {
            result.push(Plant {
                cell,
                crop: Crop::Vegetable,
            });
            ve -= 1
        }
    }
    result
}
fn animal_plan(p: &Player, animal: Animal, quantity: u32) -> AnimalPlan {
    let mut plan = AnimalPlan::discard_all(animal, 0);
    let mut remaining = quantity;
    if remaining > 0 && p.pet.is_none() {
        plan.pet = true;
        remaining -= 1;
    }
    for pasture in &p.pastures {
        if remaining == 0 {
            break;
        }
        if pasture.animal.is_none() || pasture.animal == Some(animal) {
            let free = p.pasture_capacity(pasture) - pasture.quantity;
            let add = remaining.min(free);
            if add > 0 {
                plan.to_pastures.push((pasture.id, add));
                remaining -= add
            }
        }
    }
    for (cell_index, cell) in p.farm.iter().enumerate() {
        if remaining == 0 {
            break;
        }
        if matches!(cell.tile, Tile::Empty) && cell.stable && cell.stable_animal.is_none() {
            plan.to_stables.push(cell_index);
            remaining -= 1
        }
    }
    // If none of the animals can be kept, a fire lets the player convert rather than lose them.
    if remaining > 0 && p.improvements.iter().any(|i| i.is_cooking()) {
        plan.cook = remaining;
        remaining = 0
    }
    plan.discard = remaining;
    plan
}
fn improvement_choice(obs: &Observation) -> Option<Improvement> {
    let p = &obs.public.players[obs.player_id];
    let market = &obs.public.improvement_market;
    market
        .iter()
        .copied()
        .filter(|&i| {
            // Same discounted affordability the engine's can_buy uses, so a
            // legal MajorImprovement space always has a purchasable detail.
            crate::engine::affordable(p, i)
                || matches!(i, Improvement::Hearth4 | Improvement::Hearth5)
                    && p.improvements
                        .iter()
                        .any(|x| matches!(x, Improvement::Fireplace2 | Improvement::Fireplace3))
        })
        .max_by_key(|i| match i {
            Improvement::Hearth4 | Improvement::Hearth5 => 50,
            Improvement::Fireplace2 | Improvement::Fireplace3 => 45,
            Improvement::Well => 40,
            Improvement::ClayOven | Improvement::StoneOven => 35,
            _ => 20,
        })
}
/// Value weight for one on-play choice option (C007 刈り鋏): resolve a
/// per-animal option against the owner's current state and compare it with
/// the plain amount. Deterministic: the lower index wins ties.
fn choice_gain_value(
    player: &crate::model::Player,
    option: &crate::cards::ChoiceGainOption,
) -> u32 {
    match option.per_animal {
        Some(animal) => option.amount.saturating_mul(player.animal_count(animal)),
        None => option.amount,
    }
}

/// Index of the best on-play choice option for the owner.
fn choice_index(
    player: &crate::model::Player,
    options: &[crate::cards::ChoiceGainOption],
) -> usize {
    let mut best = 0usize;
    let mut best_value = u32::MIN;
    for (i, option) in options.iter().enumerate() {
        let value = choice_gain_value(player, option);
        if i == 0 || value > best_value {
            best = i;
            best_value = value;
        }
    }
    best
}

impl DetailPolicy for SimpleDetails {
    fn produce(&self, obs: &Observation, space: Space) -> Option<Action> {
        let p = &obs.public.players[obs.player_id];
        let card_set = crate::cards::CardSet {
            definitions: obs.card_definitions.clone(),
        };
        let detail = match space {
            Space::Lessons | Space::ExtraLessons => {
                let count = p
                    .played_cards
                    .iter()
                    .filter(|id| {
                        obs.card_definitions
                            .iter()
                            .any(|c| c.id == **id && c.kind == crate::cards::CardKind::Occupation)
                    })
                    .count();
                let fee = if space == Space::Lessons {
                    u32::from(count > 0)
                } else if obs.public.players.len() == 3 || count >= 2 {
                    2
                } else {
                    1
                };
                let card_id = obs
                    .hand
                    .iter()
                    .find(|id| {
                        obs.card_definitions.iter().any(|card| {
                            card.id == **id
                                && card.kind == crate::cards::CardKind::Occupation
                                && crate::cards::can_play(&card_set, card, p, obs.public.round, fee)
                        })
                    })?
                    .clone();
                Detail::PlayOccupation { card_id }
            }
            Space::Forest
            | Space::ClayPit
            | Space::ReedBank
            | Space::Fishing
            | Space::DayLaborer
            | Space::GrainSeeds
            | Space::VegetableSeeds
            | Space::MeetingPlace
            | Space::WesternQuarry
            | Space::EasternQuarry
            | Space::Copse
            | Space::Grove
            | Space::Hollow
            | Space::TravelingPlayers
            | Space::FamilyGrowthWithRoom
            | Space::ModestFamilyGrowth
            | Space::FamilyGrowthNoRoom => Detail::None,
            Space::ResourceMarket => {
                if obs.public.players.len() == 3 {
                    Detail::MarketChoice(Good::Stone)
                } else {
                    Detail::None
                }
            }
            Space::Farmland => Detail::Plow {
                cell: empty_field_target(p)?,
            },
            Space::FarmExpansion => {
                if p.supply.reed >= 2
                    && p.supply.get(match p.house {
                        House::Wood => Good::Wood,
                        House::Clay => Good::Clay,
                        House::Stone => Good::Stone,
                    }) >= 5
                {
                    if let Some(cell) = empty_room_target(p) {
                        Detail::Build {
                            rooms: vec![cell],
                            stables: vec![],
                        }
                    } else if p.supply.wood >= 2 {
                        Detail::Build {
                            rooms: vec![],
                            stables: vec![stable_target(p)?],
                        }
                    } else {
                        return None;
                    }
                } else if p.supply.wood >= 2 {
                    Detail::Build {
                        rooms: vec![],
                        stables: vec![stable_target(p)?],
                    }
                } else {
                    return None;
                }
            }
            Space::Fencing => {
                if let Some(group) = small_pasture(p) {
                    Detail::Fence {
                        pastures: vec![group],
                    }
                } else if let Some(pastures) = crate::engine::split_candidate(p) {
                    Detail::Fence { pastures }
                } else if let Some((pastures, occupancy)) =
                    crate::engine::split_occupied_candidate(p)
                {
                    Detail::FenceDetailed {
                        pastures,
                        occupancy,
                    }
                } else {
                    let (pastures, occupancy) = crate::engine::general_split_candidate(p)?;
                    Detail::FenceDetailed {
                        pastures,
                        occupancy,
                    }
                }
            }
            Space::GrainUtilization => {
                let plant = plantings(p, None);
                // Baking is allocated by the replaceable detail policy, not
                // chosen by rule application. A default policy may still use
                // the oven(s) and then one cooking card.
                let unplanted_grain = p
                    .supply
                    .grain
                    .saturating_sub(plant.iter().filter(|x| x.crop == Crop::Grain).count() as u32);
                let mut remaining = if plant.is_empty() { unplanted_grain } else { 0 };
                let mut baking = Vec::new();
                for (improvement, cap) in [(Improvement::ClayOven, 1), (Improvement::StoneOven, 2)]
                {
                    if p.improvements.contains(&improvement) && remaining > 0 {
                        let grain = remaining.min(cap);
                        remaining -= grain;
                        baking.push(BakeAllocation { improvement, grain });
                    }
                }
                if remaining > 0 {
                    if let Some(improvement) = p
                        .improvements
                        .iter()
                        .copied()
                        .find(|i| matches!(i, Improvement::Hearth4 | Improvement::Hearth5))
                        .or_else(|| {
                            p.improvements.iter().copied().find(|i| {
                                matches!(i, Improvement::Fireplace2 | Improvement::Fireplace3)
                            })
                        })
                    {
                        baking.push(BakeAllocation {
                            improvement,
                            grain: remaining,
                        });
                    }
                }
                Detail::SowDetailed {
                    plantings: plant,
                    baking,
                }
            }
            Space::Cultivation => {
                let cell = empty_field_target(p);
                let plant = plantings(p, cell);
                Detail::Cultivate {
                    plow: cell,
                    plantings: plant,
                }
            }
            Space::MajorImprovement => {
                if let Some(improvement) = improvement_choice(obs) {
                    Detail::Buy {
                        improvement,
                        instant_bake: 0,
                    }
                } else {
                    let card_id = obs
                        .hand
                        .iter()
                        .find(|id| {
                            obs.card_definitions.iter().any(|card| {
                                card.id == **id
                                    && card.kind == crate::cards::CardKind::MinorImprovement
                                    && crate::cards::can_play(
                                        &card_set,
                                        card,
                                        p,
                                        obs.public.round,
                                        0,
                                    )
                            })
                        })?
                        .clone();
                    // C007-style choice cards: pick the better on-play gain
                    // option (per-animal amounts resolve against the owner's
                    // current state) and pass the choice as action data.
                    if let Some(card) = card_set.definition(&card_id) {
                        if let Some(crate::cards::CardEffect::OnPlayChoiceGain { options }) =
                            card.effects.iter().find(|e| {
                                matches!(e, crate::cards::CardEffect::OnPlayChoiceGain { .. })
                            })
                        {
                            let option = choice_index(p, options);
                            return Some(Action {
                                space,
                                detail: Detail::PlayMinorChoice { card_id, option },
                            });
                        }
                    }
                    Detail::PlayMinor { card_id }
                }
            }
            Space::HouseRedevelopment | Space::FarmRedevelopment => Detail::Renovate {
                bonus_improvement: None,
                instant_bake: 0,
                pastures: vec![],
            },
            Space::SheepMarket | Space::PigMarket | Space::CattleMarket => {
                let species = match space {
                    Space::SheepMarket => Animal::Sheep,
                    Space::PigMarket => Animal::Boar,
                    _ => Animal::Cattle,
                };
                let count = obs
                    .public
                    .animal_accumulations
                    .get(&space)
                    .copied()
                    .unwrap_or(0);
                Detail::Animals(animal_plan(p, species, count))
            }
            Space::AnimalMarket => Detail::Animals(animal_plan(p, Animal::Sheep, 1)),
        };
        Some(Action { space, detail })
    }
}

/// A card-aware detail policy for M7 search. Unlike SimpleDetails (which
/// takes the first playable card), this scores only the observing player's
/// known hand and makes a deterministic choice. It never reads hidden Game.
pub struct CardChoiceDetails;

fn card_good_value(good: Good) -> i64 {
    match good {
        Good::Food => 3,
        Good::Grain | Good::Vegetable => 4,
        Good::Wood | Good::Clay | Good::Reed | Good::Stone => 3,
    }
}

fn card_animal_value(animal: Animal) -> i64 {
    match animal {
        Animal::Sheep => 4,
        Animal::Boar => 5,
        Animal::Cattle => 6,
    }
}

/// Score the card's actual immediate resource/point change after payments,
/// then add a bounded forecast for explicitly recurring or passive effects.
/// This avoids treating conditional on-play rewards as unconditional.
fn card_priority(
    set: &crate::cards::CardSet,
    card: &crate::cards::CardDefinition,
    player: &Player,
    fee: u32,
    round: u8,
) -> Option<i64> {
    use crate::cards::CardEffect;
    let mut after = player.clone();
    let mut hand = vec![card.id.clone()];
    crate::cards::play(set, &mut hand, &mut after, &card.id, card.kind, round, fee).ok()?;
    let mut value = i64::from(after.card_points - player.card_points) * 6;
    for good in [
        Good::Food,
        Good::Wood,
        Good::Clay,
        Good::Reed,
        Good::Stone,
        Good::Grain,
        Good::Vegetable,
    ] {
        value += card_good_value(good)
            * (i64::from(after.supply.get(good)) - i64::from(player.supply.get(good)));
    }
    let horizon = i64::from(15u8.saturating_sub(round)).min(5);
    for effect in &card.effects {
        value += match effect {
                // Exact gains and discounted costs were simulated above.
                                CardEffect::OnPlayGain { .. }
                                                | CardEffect::OnPlayGainPerStored { .. }
                                                | CardEffect::OnPlayGainIfHouse { .. }
                                                | CardEffect::OnPlayGainIfRoomCount { .. }
                                                | CardEffect::OnPlayGainPerImprovementPlayed { .. }
                                                | CardEffect::OnPlayGainEachWithAtLeast { .. }
                                                | CardEffect::Prerequisite { .. } => 0,
                #[cfg(test)]
                // Test-only probe; never part of a real evaluated pool.
                CardEffect::PendingProbe { .. } => 0,
                CardEffect::AfterWorkerGain { good, amount, .. }
                | CardEffect::StartRoundGain { good, amount, .. }
                | CardEffect::StartRoundGainIfFoodAtLeastRound { good, amount, .. }
                // C116's wood-per-food-cost is occupation-cost dependent; the
                                // heuristic treats one grant per remaining round as an upper bound.
                                | CardEffect::AfterOccupationGainPerFoodCost { good, amount, .. } => {
                                    card_good_value(*good) * i64::from(*amount) * horizon
                                }
                                CardEffect::AfterWorkerGainIfOwnsAnimal { good, amount, .. } => {
                                    card_good_value(*good) * i64::from(*amount) * horizon
                                }
                                // A068 アスパラガスの贈り物: the fence-count-vs-round
                                // condition depends on farm history the heuristic cannot
                                // read; value the gain optimistically at one grant per
                                // remaining round, mirroring the sibling conditional arm.
                                CardEffect::AfterWorkerGainIfFencesAtLeastRound {
                                    good,
                                    amount,
                                    ..
                                } => {
                                    card_good_value(*good) * i64::from(*amount) * horizon
                                }
                                // A052: conditional on a standing pig-market accumulation;
                                // value at the same conditional discount as A140/E111.
                                CardEffect::AfterWorkerGainIfAnimalAccumulationAtLeast { good, amount, .. } => {
                                    card_good_value(*good) * i64::from(*amount) * horizon / 2
                                }
                                // C068 書棚: fixed gain per occupation played; similar heuristic to
                                // PerFoodCost but without food-cost scaling.
                                | CardEffect::AfterOccupationGain { good, amount, .. } => {
                                    card_good_value(*good) * i64::from(*amount) * horizon
                                }
                                CardEffect::AfterWorkerGainIfUsed { good, amount, .. }
                                                                | CardEffect::HarvestFieldGain { good, amount, .. }
                | CardEffect::AfterClayConstructionGain { good, amount }
                // E111's halves fire only while the owner stays minor-free; the
                // heuristic accepts the conditional gain optimistically.
                | CardEffect::StartRoundGainIfNoMinorPlayed { good, amount, .. }
                                | CardEffect::HarvestFieldGainIfNoMinorPlayed { good, amount, .. } => {
                                    card_good_value(*good) * i64::from(*amount) * horizon / 2
                                }
                                // A140 ショベル運搬人: the food amount equals the OTHER
                                // accumulation's current clay count, which this heuristic
                                // cannot read; approximate with a typical mid-game
                                // accumulation (2 clay) at the same conditional discount.
                                CardEffect::AfterWorkerGainOtherAccumulationFood { good, .. } => {
                                    card_good_value(*good) * 2 * horizon / 2
                                }
                // OnPlayGainPerCount (ch3 OnPlayGainPerCount chapter): the exact
                // per-count quantity needs board state this heuristic does not
                // have, so use a per-kind typical count at play time.
                CardEffect::OnPlayGainPerCount {
                    count_kind,
                    good,
                    amount,
                    ..
                } => {
                    let avg_count: i64 = match count_kind {
                        crate::cards::CountKind::Occupations => 2, // ~2 occupations mid-game
                        crate::cards::CountKind::ClayRooms => 1, // ~1 clay room after upgrade
                        crate::cards::CountKind::Improvements => 3, // ~3 improvements (base + 2 minor)
                        crate::cards::CountKind::FamilyMembers => 3, // ~3 family members
                        crate::cards::CountKind::MajorImprovementsBottomRow => 1,
                        crate::cards::CountKind::SownFields => 2, // ~2 sown fields
                    };
                    card_good_value(*good) * i64::from(*amount) * avg_count
                }
                CardEffect::Passive { .. } => horizon * 5,
                                CardEffect::EndGameBonus { .. } => 12,
                                CardEffect::AfterHarvestBakeGain { amount } => i64::from(*amount) * horizon,
                                                                CardEffect::AfterHarvestGain { good, amount, .. }
                                | CardEffect::AfterHarvestGainIfAtLeast { good, amount, .. } => {
                                    card_good_value(*good) * i64::from(*amount) * horizon
                                }
                                // C066: the conditional choice grants the primary or the
                                // upgrade gain; value the better of the two at the same
                                // optimistic discount as the sibling after-harvest arms.
                                CardEffect::AfterHarvestChooseGainIfAtLeast {
                                    good,
                                    amount,
                                    upgrade_gain,
                                    upgrade_amount,
                                    ..
                                } => {
                                    let primary = card_good_value(*good) * i64::from(*amount);
                                    let upgrade = card_good_value(*upgrade_gain) * i64::from(*upgrade_amount);
                                    primary.max(upgrade) * horizon
                                }
                                // A118: the exchange is plan-driven and capped at 2 buys per
                                                                // harvest, so value one purchase per remaining harvest at a
                                                                // 50% discount (mirrors the sibling conditional arms).
                                                                CardEffect::HarvestFieldOptionalExchange { gain, gain_amount, .. } => {
                                                                                                                                    card_good_value(*gain) * i64::from(*gain_amount) * horizon / 2
                                                                                                                                }
                                                                                                                                // harvest_feeding automatic gains (D012): fixed per-harvest grant
                                                                                                                                CardEffect::HarvestFeedingGain { good, amount, .. } => {
                                                                                                                                    let good_val = good.map(card_good_value).unwrap_or(0);
                                                                                                                                    good_val * i64::from(*amount) * horizon
                                                                                                                                }
                                                                                                                                // harvest_feeding optional exchanges (A062, C055, C059, C063, D062, D084): plan-driven, conditional
                                                                CardEffect::HarvestFeedingOptionalExchange { options, .. } => {
                                                                    let max_food = options.iter().map(|o| o.receive_food).max().unwrap_or(0);
                                                                    let max_points = options.iter().map(|o| o.receive_points).max().unwrap_or(0);
                                                                    let has_animal = options.iter().any(|o| o.receive_animal);
                                                                    let animal_val = if has_animal { card_animal_value(Animal::Sheep) } else { 0 };
                                                                    (card_good_value(Good::Food) * i64::from(max_food) + animal_val + i64::from(max_points)) * horizon / 2
                                                                }
                                                                // New variants for market followup cards (PR #393)
                                // New variants for market followup cards (PR #393)
                                // New variants for after_worker_space simple gain cards (B017, B047, B051, B060, B063)
                                CardEffect::AfterWorkerSpendPlowField { .. } => horizon * 4, // plowing is valuable
                                CardEffect::AfterWorkerScheduleFood { rounds, .. } => {
                                    // Each use places 1 food on each of N future rounds
                                    card_good_value(Good::Food) * i64::from(*rounds) * horizon / 4
                                }
                                CardEffect::AfterWorkerGainPerAnimal { good, per, .. } => {
                                    // Approximate with 2 animals mid-game
                                    card_good_value(*good) * i64::from(*per) * 2 * horizon / 4
                                }
                                CardEffect::AfterWorkerPayToScheduleFood { pay_good, pay_amount, rounds, .. } => {
                                    // Net value: food gained minus grain paid
                                    let food_value = card_good_value(Good::Food) * i64::from(*rounds);
                                    let pay_cost = card_good_value(*pay_good) * i64::from(*pay_amount);
                                    (food_value - pay_cost) * horizon / 4
                                }
                                CardEffect::AfterWorkerOptionalExchange { give, give_amount, receive, receive_amount, .. } => {
                                    // Net value per use
                                    let gain = card_good_value(*receive) * i64::from(*receive_amount);
                                    let cost = card_good_value(*give) * i64::from(*give_amount);
                                    (gain - cost) * horizon / 4
                                }
                                CardEffect::AfterAnyWorkerGain { good, amount, .. }
                                | CardEffect::AfterAnyWorkerOthersGain { good, amount, .. } => {
                                    // Fires on any player's use of the space; approximate with per-round expectation
                                    card_good_value(*good) * i64::from(*amount) * horizon / 4
                                }
                                CardEffect::AfterAnyWorkerGainByCountIfNotActor {
                                    base_good,
                                    base_amount,
                                    ..
                                } => {
                                    // B143: fires on any player's use of Hollow;
                                    // value the base clay gain at per-round expectation.
                                    card_good_value(*base_good) * i64::from(*base_amount) * horizon / 4
                                }
                                CardEffect::HarvestStartGainIfFields { good, amount, .. } => {
                                                                    // Conditional on field count; optimistic half-weight
                                                                    card_good_value(*good) * i64::from(*amount) * horizon / 2
                                                                }
                                                                                                                                // B-deck on-play chapter 1 (B002/B003/B005/B008/B009)
                                                                                                                                CardEffect::OnPlayBuildPasture { .. } => {
                                                                                                                                    // Free 1-space pasture: ~1-2 wood value + animal capacity
                                                                                                                                    horizon * 3
                                                                                                                                }
                                                                                                                                CardEffect::OnPlayDrawPlayOrPassOccupation { food_cost } => {
                                                                                                                                    // Draw occupation, play for food_cost or pass left.
                                                                                                                                    // Heuristic: ~1 occupation value - food_cost
                                                                                                                                    10 - i64::from(*food_cost)
                                                                                                                                }
                                                                                                                                CardEffect::OnPlayGainByHandOccupationCount { bands, .. } => {
                                                                                                                                    // Gain 1 resource based on hand occupation count.
                                                                                                                                    // Typical mid-game: 3-4 occupations -> stone (value ~2)
                                                                                                                                    card_good_value(bands[0].1) // first band's good as baseline
                                                                                                                                }
                                                                                                                                CardEffect::OnPlayExchangeGain {
                                                                                                                                    give,
                                                                                                                                    receive,
                                                                                                                                    give_amount,
                                                                                                                                    receive_amount,
                                                                                                                                    ..
                                                                                                                                } => {
                                                                                                                                    // Exchange give_amount of give for receive_amount of receive
                                                                                                                                    (card_good_value(*receive) * i64::from(*receive_amount))
                                                                                                                                        .saturating_sub(card_good_value(*give) * i64::from(*give_amount))
                                                                                                                                }
                                                                                                                                CardEffect::OnPlayGainOrExchangeAnimal {
                                                                                                                                    gain_good,
                                                                                                                                    gain_amount,
                                                                                                                                    pay_good,
                                                                                                                                    pay_amount,
                                                                                                                                    animal,
                                                                                                                                    ..
                                                                                                                                } => {
                                                                                                                                    // Gain gain_good or pay pay_good for animal
                                                                                                                                    // Heuristic: max of (gain value, animal value - pay cost)
                                                                                                                                    let gain_val = card_good_value(*gain_good) * i64::from(*gain_amount);
                                                                                                                                    let exchange_val = card_animal_value(*animal)
                                                                                                                                        .saturating_sub(card_good_value(*pay_good) * i64::from(*pay_amount));
                                                                                                                                    gain_val.max(exchange_val)
                                                                                                                                }
                                // C083/C147 (main b876a46/f03d65b): animal gain variants from main.
                                CardEffect::HarvestFieldGainIfGrainHarvestedAtLeast { good, amount, .. } => {
                                    // D065 麦ふるい: conditional on harvesting 2+ grain this
                                    // field phase; optimistic half-weight like the sibling
                                    // conditional arms.
                                    card_good_value(*good) * i64::from(*amount) * horizon / 2
                                }
                                CardEffect::OnPlayGainAnimal { animal, amount, .. }
                                | CardEffect::AfterWorkerGainAnimal { animal, amount, .. } => {
                                    card_animal_value(*animal) * i64::from(*amount) * horizon
                                }
                                // C006/C007 on-play effects: stone placement and choice gain
                                // are resolved at play time; heuristic assigns neutral value.
                                CardEffect::OnPlayChoiceGain { .. } => 0,
                                CardEffect::OnPlayAddStoneToEmptyFields { .. } => 0,
                                // PostPlayPassLeft is a zone move with no valuation.
                                CardEffect::PostPlayPassLeft => 0,
                                CardEffect::AfterImprovementPlayedGain { good, amount, .. } => {
                                    card_good_value(*good) * i64::from(*amount) * horizon
                                }
                                // Chapter 1 round-start cards: complex conditional effects not evaluated by heuristic
                                CardEffect::StartRoundPlowFieldIfHouse { .. }
                                | CardEffect::StartRoundCopyUnusedRoundSpace { .. }
                                | CardEffect::StartRoundPlayOccupationIfLessonsUnderused { .. }
                                | CardEffect::StartRoundPlayCardIfStoneHouse { .. }
                                | CardEffect::StartRoundGainIfGoodsOnRoundSpaces { .. }
                                | CardEffect::StartRoundExchangeForFoodAndPoints { .. }
                                | CardEffect::StartRoundPlaceFoodOnNextRoundSpaces { .. } => 0,
                                // B114/A157/A049/C067: conditional round-start gains
                                // (B114 family+rooms, A157 Lessons occupants, A049
                                // accumulation grew, C067 non-harvest + pasture
                                // animals). Same optimistic per-remaining-round
                                // discount as the E111 conditional arm.
                                CardEffect::StartRoundGainIfFamilyAndRooms { good, amount, .. }
                                | CardEffect::StartRoundGainIfLessonsOccupants { good, amount, .. }
                                | CardEffect::StartRoundGainIfAccumulationGrew { good, amount, .. }
                                | CardEffect::StartRoundGainIfAccumulationAtLeast { good, amount, .. }
                                | CardEffect::StartRoundGainIfNoHarvestAndAnimals {
                                                                                                    good,
                                                                    amount,
                                                                    ..
                                                                } => card_good_value(*good) * i64::from(*amount) * horizon,
                                                                // A001/C002/E001: a free stable carries the same
                                                                // ongoing value as a built one (~2 wood saved +
                                                                // 2-animal capacity); E001's 3-stable "up to"
                                                                // counts each site.
                                                                CardEffect::OnPlayBuildStable { amount, .. } => {
                                                                    card_good_value(crate::model::Good::Wood)
                                                                        * i64::from(*amount)
                                                                        * 2
                                                                }
                                                            };
    }
    Some(value)
}

fn preferred_card(
    obs: &Observation,
    kind: crate::cards::CardKind,
    fee: u32,
) -> Option<(String, i64)> {
    let player = obs.public.players.get(obs.player_id)?;
    let set = crate::cards::CardSet {
        definitions: obs.card_definitions.clone(),
    };
    obs.hand
        .iter()
        .filter_map(|id| {
            let card = set.definition(id)?;
            if card.kind != kind
                || !crate::cards::can_play(&set, card, player, obs.public.round, fee)
            {
                return None;
            }
            Some((
                id,
                card_priority(&set, card, player, fee, obs.public.round)?,
            ))
        })
        .max_by(|a, b| a.1.cmp(&b.1).then_with(|| b.0.cmp(a.0)))
        .map(|(id, priority)| (id.clone(), priority))
}

impl DetailPolicy for CardChoiceDetails {
    fn produce(&self, obs: &Observation, space: Space) -> Option<Action> {
        use crate::cards::CardKind;
        match space {
            Space::Lessons | Space::ExtraLessons => {
                let p = obs.public.players.get(obs.player_id)?;
                let played = p
                    .played_cards
                    .iter()
                    .filter(|id| {
                        obs.card_definitions
                            .iter()
                            .any(|card| card.id == **id && card.kind == CardKind::Occupation)
                    })
                    .count();
                let fee = if space == Space::Lessons {
                    u32::from(played > 0)
                } else if obs.public.players.len() == 3 || played >= 2 {
                    2
                } else {
                    1
                };
                let (card_id, _) = preferred_card(obs, CardKind::Occupation, fee)?;
                return Some(Action {
                    space,
                    detail: Detail::PlayOccupation { card_id },
                });
            }
            Space::MeetingPlace | Space::MajorImprovement => {
                if let Some((card_id, priority)) =
                    preferred_card(obs, CardKind::MinorImprovement, 0)
                {
                    if priority >= 12 {
                        // Choice cards (C007) need the choice as action data.
                        if let Some(card) = (crate::cards::CardSet {
                            definitions: obs.card_definitions.clone(),
                        })
                        .definition(&card_id)
                        {
                            if let Some(crate::cards::CardEffect::OnPlayChoiceGain { .. }) =
                                card.effects.iter().find(|e| {
                                    matches!(e, crate::cards::CardEffect::OnPlayChoiceGain { .. })
                                })
                            {
                                return None;
                            }
                        }
                        return Some(Action {
                            space,
                            detail: Detail::PlayMinor { card_id },
                        });
                    }
                }
            }
            Space::FamilyGrowthWithRoom | Space::ModestFamilyGrowth => {
                if let Some((card_id, priority)) =
                    preferred_card(obs, CardKind::MinorImprovement, 0)
                {
                    if priority >= 12 {
                        return Some(Action {
                            space,
                            detail: Detail::GrowWithMinor { card_id },
                        });
                    }
                }
            }
            _ => {}
        }
        SimpleDetails.produce(obs, space)
    }
}

pub fn play_to_end(
    game: &mut Game,
    space_policy: &dyn SpacePolicy,
    detail_policy: &dyn DetailPolicy,
) -> Result<(), String> {
    play_to_end_with_harvest_policy(game, space_policy, detail_policy, &SimpleHarvestDetails)
}
pub fn play_to_end_with_harvest_policy(
    game: &mut Game,
    space_policy: &dyn SpacePolicy,
    detail_policy: &dyn DetailPolicy,
    harvest_policy: &dyn HarvestDetailPolicy,
) -> Result<(), String> {
    play_to_end_observed(game, space_policy, detail_policy, harvest_policy, |_, _| {})
}

/// Same loop as `play_to_end_with_harvest_policy`, but reports the wall-clock
/// duration of each *complete* decision (action-space selection plus detail
/// generation) to `on_decision` as `(acting_player, nanoseconds)`. The observer
/// never changes game state, choices, or RNG streams, so action-only replay
/// stays identical to the unobserved path.
pub fn play_to_end_observed(
    game: &mut Game,
    space_policy: &dyn SpacePolicy,
    detail_policy: &dyn DetailPolicy,
    harvest_policy: &dyn HarvestDetailPolicy,
    mut on_decision: impl FnMut(usize, u64),
) -> Result<(), String> {
    play_to_end_with_budget(
        game,
        space_policy,
        detail_policy,
        harvest_policy,
        None,
        |outcome| on_decision(outcome.player, outcome.elapsed_ns),
    )
}

/// The guaranteed-legal automatic detail generation used as a fallback.
///
/// `SimpleDetails` returns one legal action for every available space, so a
/// decision never ends without a legal action even when the configured detail
/// policy declines or the budget is already spent.
pub fn fallback_detail(obs: &Observation, space: Space) -> Option<Action> {
    SimpleDetails.produce(obs, space)
}

/// Which seats a whole-decision budget applies to (#167 A review P1).
///
/// A wall-clock budget makes a decision machine-dependent, so applying it to a
/// seat that is not being measured silently changes the opponent's behaviour and
/// can even change a replay. Only the seat under evaluation may be budgeted;
/// every other seat keeps the clock-free, reproducible path.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BudgetScope {
    /// No seat is budgeted (equivalent to no budget at all).
    NoSeats,
    /// Every seat is budgeted. Only for a game where the whole game, not one
    /// seat, is the thing being timed; never for a paired comparison.
    AllSeats,
    /// Only this seat is budgeted; all other seats keep their historical
    /// behaviour and determinism.
    FocusSeat(usize),
}

/// A resolved budget plus the seats it may apply to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BudgetedDecisions {
    pub spec: DecisionBudgetSpec,
    pub scope: BudgetScope,
}

impl BudgetedDecisions {
    /// The evaluation shape: only the focus seat is time-budgeted.
    pub fn focus_only(spec: DecisionBudgetSpec, seat: usize) -> Self {
        Self {
            spec,
            scope: BudgetScope::FocusSeat(seat),
        }
    }

    pub fn all_seats(spec: DecisionBudgetSpec) -> Self {
        Self {
            spec,
            scope: BudgetScope::AllSeats,
        }
    }

    pub fn applies_to(&self, seat: usize) -> bool {
        match self.scope {
            BudgetScope::NoSeats => false,
            BudgetScope::AllSeats => true,
            BudgetScope::FocusSeat(focus) => focus == seat,
        }
    }
}

/// Play a game to the end under a **whole-decision** wall-clock budget (#167 A).
///
/// Unlike [`play_to_end_observed`], this bounds selection *and* detail
/// generation with one budget: the search receives
/// [`crate::decision_budget::DecisionBudget::search_deadline`] (the decision
/// deadline minus the detail reserve), and details receive the decision
/// deadline. When the budget is
/// already spent by the time details would start, or the detail policy declines,
/// the decision falls back to [`fallback_detail`] instead of failing the game.
///
/// The budget applies **only to the seats in `budget.scope`** (#167 A review
/// P1). Every other seat takes the pre-#167 path exactly: no clock, no fallback
/// substitution, and the same reproducible choices for a fixed RNG seed. A
/// paired comparison must therefore use [`BudgetedDecisions::focus_only`], never
/// [`BudgetedDecisions::all_seats`].
///
/// `decision_budget = None` keeps the historical behaviour for every seat.
/// `simulation-capped` evaluation must use `None` so replays stay identical.
///
/// Every decision is reported to `on_decision`, including the fallback flags and
/// the measured overshoot (budgeted seats only). The overshoot is a measurement:
/// a running function cannot be preempted, so no real-time guarantee is made.
pub fn play_to_end_with_budget(
    game: &mut Game,
    space_policy: &dyn SpacePolicy,
    detail_policy: &dyn DetailPolicy,
    harvest_policy: &dyn HarvestDetailPolicy,
    decision_budget: Option<BudgetedDecisions>,
    mut on_decision: impl FnMut(DecisionOutcome),
) -> Result<(), String> {
    for _ in 0..320 {
        if game.public.finished {
            return Ok(());
        }
        let acting = game.public.active_player;
        let obs = game.observation(acting)?;
        let legal = game.legal_spaces();
        if legal.is_empty() {
            return Err(format!(
                "no legal move in round {} for player {}",
                game.public.round, acting
            ));
        }
        // Deterministic per-decision RNG: the policy never advances the rule engine RNG.
        // This preserves action-only replay, and reveals neither hidden deck nor round RNG.
        let decision_index = game.log.events.len() as u64;
        let mut choice_rng = ChaCha8Rng::seed_from_u64(
            game.log.action_seed ^ decision_index.wrapping_mul(0x9E37_79B9_7F4A_7C15),
        );
        // Only the seats in scope get a clock (#167 A review P1). Everyone else
        // takes the pre-#167 path, so an opponent's choices cannot drift with the
        // wall clock and a replay cannot change because a budget was configured.
        let seat_budget = decision_budget.filter(|budget| budget.applies_to(acting));
        let (action, outcome) = resolve_decision(
            &obs,
            &legal,
            space_policy,
            detail_policy,
            &mut choice_rng,
            seat_budget.map(|budget| budget.spec),
        )?;
        on_decision(outcome);
        let chosen = action.space;
        game.apply_with_harvest_policy(action, harvest_policy)
            .map_err(|e| format!("policy action {:?} failed: {}", chosen, e))?;
    }
    if game.public.finished {
        Ok(())
    } else {
        Err("move limit exceeded".into())
    }
}

/// Resolve exactly one decision under an optional whole-decision budget.
///
/// This is the single implementation of "select a space, then generate its
/// details, inside one wall-clock window". The play loop and the profiling
/// binary both call it, so a measurement can never come from a different code
/// path than the one that runs games.
///
/// Without a budget the historical contract is preserved: a refusing policy is
/// an error, and nothing is measured or substituted. With `Some(spec)` the
/// returned action is always legal even when the search or the detail policy
/// gives up.
pub fn resolve_decision(
    obs: &Observation,
    legal: &[Space],
    space_policy: &dyn SpacePolicy,
    detail_policy: &dyn DetailPolicy,
    rng: &mut ChaCha8Rng,
    decision_budget: Option<DecisionBudgetSpec>,
) -> Result<(Action, DecisionOutcome), String> {
    if legal.is_empty() {
        return Err("no legal move".into());
    }
    let acting = obs.player_id;
    let budget = decision_budget.map(|spec| spec.resolve());
    let decision_start = Instant::now();
    let selected = space_policy.select_with_deadline(
        obs,
        legal,
        rng,
        budget.map(|budget| budget.search_deadline()),
    );
    let selection_fallback = selected.is_none();
    let chosen = match selected {
        Some(space) => space,
        // Without a budget the historical contract stands: a policy that
        // refuses is an error, not something to paper over. With a budget the
        // decision must still end in a legal action, and `legal` is non-empty,
        // so the first legal space is the documented last resort (search
        // policies already return their own evaluated fallback before returning
        // `None`).
        None if budget.is_some() => legal[0],
        None => return Err("policy refused selection".into()),
    };
    let budget_exhausted_before_details = budget.map(|b| b.is_expired()).unwrap_or(false);
    let detail_start = Instant::now();
    let produced = if budget_exhausted_before_details {
        // The search used the whole budget: hand the decision to the cheap,
        // guaranteed-legal detail generator instead of letting a slow detail
        // policy run past the ceiling.
        None
    } else {
        detail_policy.produce_with_deadline(obs, chosen, budget.map(|budget| budget.deadline()))
    };
    let detail_fallback = produced.is_none();
    let detail_declined = detail_fallback && !budget_exhausted_before_details;
    let action = match produced {
        Some(action) => action,
        None if budget.is_some() => fallback_detail(obs, chosen)
            .ok_or_else(|| format!("no legal detail for {:?} for player {}", chosen, acting))?,
        None => return Err("detail policy declined".into()),
    };
    let detail_ns = detail_start.elapsed().as_nanos().min(u128::from(u64::MAX)) as u64;
    let elapsed_ns = decision_start
        .elapsed()
        .as_nanos()
        .min(u128::from(u64::MAX)) as u64;
    let budget_ms = budget.map(|budget| budget.limit_ms()).unwrap_or(0);
    let overshoot_ns = budget
        .map(|budget| {
            u64::try_from(budget.elapsed().saturating_sub(budget.limit()).as_nanos())
                .unwrap_or(u64::MAX)
        })
        .unwrap_or(0);
    Ok((
        action,
        DecisionOutcome {
            player: acting,
            budget_ms,
            elapsed_ns,
            overshoot_ns,
            detail_ns,
            selection_fallback,
            detail_fallback,
            detail_declined,
            budget_exhausted_before_details,
        },
    ))
}

#[cfg(test)]
mod card_choice_tests {
    use super::*;
    use crate::cards::{CardDefinition, CardEffect, CardKind};

    #[test]
    fn card_choice_uses_best_affordable_known_card_not_first() {
        let game = crate::model::Game::new(crate::model::Config::default());
        let mut obs = game.observation(game.public.active_player).unwrap();
        for (id, food) in [("weak", 1), ("strong", 3)] {
            obs.card_definitions.push(CardDefinition {
                use_limit: None,
                id: id.into(),
                kind: CardKind::Occupation,
                cost: vec![],
                points: 0,
                effects: vec![CardEffect::OnPlayGain {
                    good: Good::Food,
                    amount: food,
                }],
                post_play: crate::cards::PostPlayDestination::PlayedArea,
            });
            obs.hand.push(id.into());
        }
        let action = CardChoiceDetails.produce(&obs, Space::Lessons).unwrap();
        assert_eq!(
            action.detail,
            Detail::PlayOccupation {
                card_id: "strong".into()
            }
        );
        assert_eq!(
            SimpleDetails.produce(&obs, Space::Lessons).unwrap().detail,
            Detail::PlayOccupation {
                card_id: "weak".into()
            }
        );
    }

    #[test]
    fn card_choice_does_not_count_unmet_conditional_gains() {
        use crate::cards::{CardDefinition, CardEffect, CardKind};
        let game = crate::model::Game::new(crate::model::Config::default());
        let mut obs = game.observation(game.public.active_player).unwrap();
        obs.card_definitions = vec![
            CardDefinition {
                use_limit: None,
                id: "bait".into(),
                kind: CardKind::Occupation,
                cost: vec![],
                points: 0,
                effects: vec![CardEffect::OnPlayGainPerStored {
                    stored: Good::Wood,
                    divisor: 2,
                    good: Good::Food,
                    amount: 5,
                }],
                post_play: crate::cards::PostPlayDestination::PlayedArea,
            },
            CardDefinition {
                use_limit: None,
                id: "house_bait".into(),
                kind: CardKind::Occupation,
                cost: vec![],
                points: 0,
                effects: vec![CardEffect::OnPlayGainIfHouse {
                    house: House::Stone,
                    rooms: 2,
                    good: Good::Food,
                    amount: 5,
                }],
                post_play: crate::cards::PostPlayDestination::PlayedArea,
            },
            CardDefinition {
                use_limit: None,
                id: "sure_food".into(),
                kind: CardKind::Occupation,
                cost: vec![],
                points: 0,
                effects: vec![CardEffect::OnPlayGain {
                    good: Good::Food,
                    amount: 1,
                }],
                post_play: crate::cards::PostPlayDestination::PlayedArea,
            },
        ];
        obs.hand = vec!["bait".into(), "house_bait".into(), "sure_food".into()];
        let action = CardChoiceDetails.produce(&obs, Space::Lessons).unwrap();
        assert_eq!(
            action.detail,
            Detail::PlayOccupation {
                card_id: "sure_food".into()
            }
        );
    }

    #[test]
    fn no_hand_means_no_occupation_and_meeting_place_fallback() {
        let game = Game::new(Config::default());
        let obs = game.observation(game.public.active_player).unwrap();
        assert!(CardChoiceDetails.produce(&obs, Space::Lessons).is_none());
        assert_eq!(
            CardChoiceDetails
                .produce(&obs, Space::MeetingPlace)
                .unwrap()
                .detail,
            Detail::None
        );
    }

    #[test]
    fn simple_details_can_choose_existing_pasture_if_no_open_stable_site() {
        let game = Game::new(Config::default());
        let mut obs = game.observation(game.public.active_player).unwrap();
        let p = &mut obs.public.players[obs.player_id];
        p.supply.wood = 2;
        for cell in &mut p.farm {
            if matches!(cell.tile, Tile::Empty) {
                cell.tile = Tile::Field {
                    crop: None,
                    remaining: 0,
                };
            }
        }
        // This observation isolates policy target selection. The rule engine
        // validates real farm topology independently on action application.
        p.farm[14].tile = Tile::Pasture { id: 0 };
        let decision = SimpleDetails.produce(&obs, Space::FarmExpansion).unwrap();
        assert_eq!(
            decision.detail,
            Detail::Build {
                rooms: vec![],
                stables: vec![14],
            }
        );
    }

    #[test]
    fn decision_observer_does_not_change_the_game() {
        let mut plain = Game::new(Config {
            players: 4,
            seed: 71,
            additional_action_space_enabled: false,
        });
        let mut observed = plain.clone();
        play_to_end(&mut plain, &GreedyPolicy, &SimpleDetails).unwrap();
        let mut max_ns = 0u64;
        let mut decisions = 0usize;
        play_to_end_observed(
            &mut observed,
            &GreedyPolicy,
            &SimpleDetails,
            &SimpleHarvestDetails,
            |_, ns| {
                max_ns = max_ns.max(ns);
                decisions += 1;
            },
        )
        .unwrap();
        assert_eq!(plain.state_hash(), observed.state_hash());
        assert!(decisions > 0);
        assert!(max_ns > 0, "whole-decision timing must be recorded");
    }
}
