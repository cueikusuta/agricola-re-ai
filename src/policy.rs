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
            i.cost().iter().all(|(g, n)| p.supply.get(*g) >= *n)
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
                                && crate::cards::can_play(&card_set, card, p, fee)
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
                            stables: vec![(0..FARM_CELLS).find(|&i| {
                                matches!(p.farm[i].tile, Tile::Empty) && !p.farm[i].stable
                            })?],
                        }
                    } else {
                        return None;
                    }
                } else if p.supply.wood >= 2 {
                    Detail::Build {
                        rooms: vec![],
                        stables: vec![(0..FARM_CELLS).find(|&i| {
                            matches!(p.farm[i].tile, Tile::Empty) && !p.farm[i].stable
                        })?],
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
                                    && crate::cards::can_play(&card_set, card, p, 0)
                            })
                        })?
                        .clone();
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
    crate::cards::play(set, &mut hand, &mut after, &card.id, card.kind, fee).ok()?;
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
            | CardEffect::Prerequisite { .. } => 0,
            CardEffect::AfterWorkerGain { good, amount, .. }
            | CardEffect::StartRoundGain { good, amount, .. } => {
                card_good_value(*good) * i64::from(*amount) * horizon
            }
            CardEffect::AfterWorkerGainIfUsed { good, amount, .. }
            | CardEffect::HarvestFieldGain { good, amount, .. }
            | CardEffect::AfterClayConstructionGain { good, amount } => {
                card_good_value(*good) * i64::from(*amount) * horizon / 2
            }
            CardEffect::Passive { .. } => horizon * 5,
            CardEffect::EndGameBonus { .. } => 12,
            CardEffect::AfterHarvestBakeGain { amount } => i64::from(*amount) * horizon,
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
            if card.kind != kind || !crate::cards::can_play(&set, card, player, fee) {
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
/// `decision_budget = None` keeps the historical behaviour exactly: no
/// deadline, no fallback substitution, and the same reproducible choices for a
/// fixed RNG seed. `simulation-capped` evaluation must use `None` so replays
/// stay identical. `Some(budget)` instead guarantees a legal action even when
/// the search or the detail policy gives up, and reports the fallback.
///
/// Every decision is reported to `on_decision`, including the fallback flags and
/// the measured overshoot. The overshoot is a measurement: a running function
/// cannot be preempted, so no real-time guarantee is made.
pub fn play_to_end_with_budget(
    game: &mut Game,
    space_policy: &dyn SpacePolicy,
    detail_policy: &dyn DetailPolicy,
    harvest_policy: &dyn HarvestDetailPolicy,
    decision_budget: Option<DecisionBudgetSpec>,
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
        let (action, outcome) = resolve_decision(
            &obs,
            &legal,
            space_policy,
            detail_policy,
            &mut choice_rng,
            decision_budget,
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
                id: id.into(),
                kind: CardKind::Occupation,
                cost: vec![],
                points: 0,
                effects: vec![CardEffect::OnPlayGain {
                    good: Good::Food,
                    amount: food,
                }],
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
            },
            CardDefinition {
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
            },
            CardDefinition {
                id: "sure_food".into(),
                kind: CardKind::Occupation,
                cost: vec![],
                points: 0,
                effects: vec![CardEffect::OnPlayGain {
                    good: Good::Food,
                    amount: 1,
                }],
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
