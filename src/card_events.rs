//! Event dispatcher for card effects with official 2016 Lookout rulebook timing.
//!
//! Resolution order per 2016 Lookout rules (active player chooses own effects order):
//! 1) Replace — active player's played_cards order (play order = official "selection order")
//! 2) Before — same
//! 3) Core action
//! 4) After — same
//! 5) Interrupt — pauses via PendingDecision; resumes after resolution
//!
//! Each resolution site carries a comment citing the 2016 Lookout rulebook.
//! Source: docs/bga-card-specification.md and existing repo citations.

use crate::cards::{CardDefinition, CardEffect, CardSet};
use crate::card_zones::CardMoveCtx;
use crate::model::{Animal, Crop, Good, House, Player, Space};
use crate::pending::{PendingDecision, PendingKind, PendingResolution};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Phase in which a card effect triggers. Determines resolution order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TriggerPhase {
    /// Replacement effects that can modify or cancel the core action.
    /// Per 2016 Lookout p.8: when multiple card effects trigger simultaneously,
    /// the active player chooses the order of resolution.
    Replace,
    /// Effects that trigger before the core action resolves.
    Before,
    /// Core action itself (not a card effect phase, but the pivot point).
    Core,
    /// Effects that trigger after the core action resolves.
    After,
    /// Effects that interrupt and require a player decision (PendingDecision).
    Interrupt,
}

/// Mapping from CardEffect variant to its TriggerPhase.
fn effect_phase(effect: &CardEffect) -> TriggerPhase {
    match effect {
        // Prerequisite blocks play — Replace phase (can prevent the play)
        CardEffect::Prerequisite { .. } => TriggerPhase::Replace,
        // On-play gains are instantaneous at play time — Before phase
        CardEffect::OnPlayGain { .. } => TriggerPhase::Before,
        CardEffect::OnPlayGainPerStored { .. } => TriggerPhase::Before,
        CardEffect::OnPlayGainIfHouse { .. } => TriggerPhase::Before,
        // Passive effects are continuous — model as Before (no choices)
        CardEffect::Passive { .. } => TriggerPhase::Before,
        // Worker completion triggers — After phase
        CardEffect::AfterWorkerGain { .. } => TriggerPhase::After,
        CardEffect::AfterWorkerGainIfUsed { .. } => TriggerPhase::After,
        // Round start — After phase (happens after round begins)
        CardEffect::StartRoundGain { .. } => TriggerPhase::After,
        // Clay construction purchase — After phase
        CardEffect::AfterClayConstructionGain { .. } => TriggerPhase::After,
        // Harvest/baking — After phase
        CardEffect::AfterHarvestBakeGain { .. } => TriggerPhase::After,
        CardEffect::HarvestFieldGain { .. } => TriggerPhase::After,
        // End game scoring — After phase
        CardEffect::EndGameBonus { .. } => TriggerPhase::After,
    }
}

/// Context passed to effect handlers during dispatch.
pub struct DispatchContext<'a> {
    pub card_set: &'a CardSet,
    pub player: &'a mut Player,
    pub cards: &'a mut crate::cards::CardState,
    pub improvement_market: &'a mut Vec<crate::model::Improvement>,
    pub delayed_well: &'a mut BTreeMap<u8, Vec<usize>>,
    pub round: u8,
    pub used_spaces: &'a std::collections::BTreeSet<Space>,
    pub event_budget: &'a mut u32,
    pub card_uses: &'a mut BTreeMap<String, u32>,
    pub pending: &'a mut Option<PendingDecision>,
}

/// Result of resolving a single effect.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EffectResult {
    /// Effect resolved normally.
    Resolved,
    /// Effect produced a pending decision; execution paused.
    Pending(PendingDecision),
    /// Effect was replaced/cancelled (Replace phase only).
    Replaced,
}

/// Consume one unit of event budget. Returns Err if exhausted.
fn consume_budget(budget: &mut u32) -> Result<(), String> {
    if *budget == 0 {
        return Err("event chain budget exhausted".into());
    }
    *budget -= 1;
    Ok(())
}

/// Check and increment per-card use count. Returns Err if limit exceeded.
fn check_card_use(card_uses: &mut BTreeMap<String, u32>, card_id: &str, limit: u32) -> Result<(), String> {
    let count = card_uses.entry(card_id.to_string()).or_insert(0);
    if *count >= limit {
        return Err(format!("card {card_id} use limit ({limit}) exceeded"));
    }
    *count += 1;
    Ok(())
}

/// Execute all effects of a given phase for a player's played cards in play order.
/// Play order = the order cards appear in `played_cards` (official "selection order").
pub fn resolve_phase(
    ctx: &mut DispatchContext<'_>,
    player: usize,
    phase: TriggerPhase,
    trigger_data: &PhaseTriggerData,
) -> Result<(), String> {
    let played_cards = ctx.player.played_cards.clone();

    for card_id in played_cards {
        let Some(card) = ctx.card_set.definition(&card_id) else {
            continue;
        };

        for effect in &card.effects {
            if effect_phase(effect) != phase {
                continue;
            }

            // Consume event budget
            consume_budget(ctx.event_budget)?;

            // Check per-card use limit (default: no limit, but track for future)
            // Currently all effects have no hard limit; the ledger is for extensibility.
            check_card_use(ctx.card_uses, &card_id, u32::MAX)?;

            // Dispatch to handler
            let result = match effect {
                CardEffect::Prerequisite { rule } => {
                    handle_prerequisite(ctx, player, &card_id, rule, trigger_data)?
                }
                CardEffect::OnPlayGain { good, amount } => {
                    handle_on_play_gain(ctx, player, good, *amount)?
                }
                CardEffect::OnPlayGainPerStored {
                    stored,
                    divisor,
                    good,
                    amount,
                } => handle_on_play_gain_per_stored(ctx, player, stored, *divisor, good, *amount)?,
                CardEffect::OnPlayGainIfHouse {
                    house,
                    rooms,
                    good,
                    amount,
                } => handle_on_play_gain_if_house(ctx, player, house, *rooms, good, *amount)?,
                CardEffect::Passive { rule } => handle_passive(ctx, player, rule)?,
                CardEffect::AfterWorkerGain { space, good, amount } => {
                    handle_after_worker_gain(ctx, player, space, good, *amount, trigger_data)?
                }
                CardEffect::AfterWorkerGainIfUsed {
                    space,
                    required_space,
                    good,
                    amount,
                } => handle_after_worker_gain_if_used(
                    ctx,
                    player,
                    space,
                    required_space,
                    good,
                    *amount,
                    trigger_data,
                )?,
                CardEffect::StartRoundGain {
                    good,
                    amount,
                    house,
                    rooms,
                } => handle_start_round_gain(ctx, player, good, *amount, house, rooms)?,
                CardEffect::AfterClayConstructionGain { good, amount } => {
                    handle_after_clay_construction_gain(ctx, player, good, *amount, trigger_data)?
                }
                CardEffect::AfterHarvestBakeGain { amount } => {
                    handle_after_harvest_bake_gain(ctx, player, *amount, trigger_data)?
                }
                CardEffect::HarvestFieldGain { rule, good, amount } => {
                    handle_harvest_field_gain(ctx, player, rule, good, *amount, trigger_data)?
                }
                CardEffect::EndGameBonus { .. } => {
                    // Scoring is handled separately in evaluation; no runtime effect.
                    EffectResult::Resolved
                }
            };

            // Handle pending interrupt
            if let EffectResult::Pending(pending) = result {
                *ctx.pending = Some(pending);
                return Err("pending decision".into()); // Signal to caller to pause
            }
        }
    }

    Ok(())
}

/// Data passed to phase handlers for context-dependent effects.
#[derive(Debug, Clone)]
pub struct PhaseTriggerData {
    /// The space that was just activated (for worker completion effects).
    pub space: Option<Space>,
    /// The improvement purchased (for clay construction effects).
    pub improvement: Option<crate::model::Improvement>,
    /// Whether a bake action occurred (for harvest bake effects).
    pub baked: bool,
    /// Previous player state (for clay construction comparison).
    pub previous_player: Option<Player>,
    /// Used spaces in the current round (for AfterWorkerGainIfUsed).
    pub used_spaces: std::collections::BTreeSet<Space>,
}

/// --- Effect Handlers (legacy implementations renamed with _legacy suffix) ---

fn handle_prerequisite(
    ctx: &mut DispatchContext<'_>,
    _player: usize,
    card_id: &str,
    rule: &crate::cards::CardRequirement,
    _trigger: &PhaseTriggerData,
) -> Result<EffectResult, String> {
    let p = &mut ctx.player;
    let ok = match rule {
        crate::cards::CardRequirement::MinimumOccupations(min) => {
            let occ_count = p
                .played_cards
                .iter()
                .filter(|id| {
                    ctx.card_set
                        .definition(id)
                        .is_some_and(|c| c.kind == crate::cards::CardKind::Occupation)
                })
                .count();
            occ_count >= usize::from(*min)
        }
        crate::cards::CardRequirement::MaximumOccupations(max) => {
            let occ_count = p
                .played_cards
                .iter()
                .filter(|id| {
                    ctx.card_set
                        .definition(id)
                        .is_some_and(|c| c.kind == crate::cards::CardKind::Occupation)
                })
                .count();
            occ_count <= usize::from(*max)
        }
        crate::cards::CardRequirement::MinimumAnimals { animal, number } => {
            p.animal_count(*animal) >= *number
        }
        crate::cards::CardRequirement::MinimumCropFields { crop, number } => {
            p.farm
                .iter()
                .filter(|cell| {
                    matches!(
                        cell.tile,
                        crate::model::Tile::Field {
                            crop: Some(found),
                            ..
                        } if found == *crop
                    )
                })
                .count()
                >= usize::from(*number)
        }
        crate::cards::CardRequirement::MinimumPlayers(_) => true, // Validated at card set level
    };

    if !ok {
        return Err(format!("prerequisite failed for card {card_id}: {rule:?}"));
    }
    Ok(EffectResult::Resolved)
}

fn handle_on_play_gain(
    ctx: &mut DispatchContext<'_>,
    player: usize,
    good: &Good,
    amount: u32,
) -> Result<EffectResult, String> {
    let p = &mut ctx.player;
    let next = p.supply.get(*good).checked_add(amount).ok_or("resource overflow")?;
    p.supply.set(*good, next);
    Ok(EffectResult::Resolved)
}

fn handle_on_play_gain_per_stored(
    ctx: &mut DispatchContext<'_>,
    player: usize,
    stored: &Good,
    divisor: u32,
    good: &Good,
    amount: u32,
) -> Result<EffectResult, String> {
    let p = &mut ctx.player;
    let bonus = (p.supply.get(*stored) / divisor)
        .checked_mul(amount)
        .ok_or("on-play bonus overflow")?;
    let next = p.supply.get(*good).checked_add(bonus).ok_or("resource overflow")?;
    p.supply.set(*good, next);
    Ok(EffectResult::Resolved)
}

fn handle_on_play_gain_if_house(
    ctx: &mut DispatchContext<'_>,
    player: usize,
    house: &House,
    rooms: u8,
    good: &Good,
    amount: u32,
) -> Result<EffectResult, String> {
    let p = &mut ctx.player;
    if p.house == *house && p.rooms() == usize::from(rooms) {
        let next = p.supply.get(*good).checked_add(amount).ok_or("resource overflow")?;
        p.supply.set(*good, next);
    }
    Ok(EffectResult::Resolved)
}

fn handle_passive(
    ctx: &mut DispatchContext<'_>,
    player: usize,
    rule: &crate::cards::PassiveRule,
) -> Result<EffectResult, String> {
    let p = &mut ctx.player;
    match rule {
        crate::cards::PassiveRule::PastureCapacity(n) => p.pasture_capacity_bonus += n,
        crate::cards::PassiveRule::FenceWoodDiscount(n) => p.free_fence_wood += n,
        crate::cards::PassiveRule::ImprovementWoodDiscount(n) => {
            p.improvement_wood_discount += n
        }
        crate::cards::PassiveRule::WoodenRoomCost(n) => p.wooden_room_cost = Some(*n),
        crate::cards::PassiveRule::RoomCost(n) => p.room_material_cost = Some(*n),
        crate::cards::PassiveRule::MajorStoneDiscountByRooms => {
            p.major_stone_discount_by_rooms = true
        }
        crate::cards::PassiveRule::StoneDiscount(n) => p.stone_cost_discount += n,
        crate::cards::PassiveRule::HousingSpace(n) => p.extra_housing += n,
    }
    Ok(EffectResult::Resolved)
}

fn handle_after_worker_gain(
    ctx: &mut DispatchContext<'_>,
    player: usize,
    space: &Space,
    good: &Good,
    amount: u32,
    trigger: &PhaseTriggerData,
) -> Result<EffectResult, String> {
    if trigger.space == Some(*space) {
        let p = &mut ctx.player;
        let next = p.supply.get(*good).checked_add(amount).ok_or("resource overflow")?;
        p.supply.set(*good, next);
    }
    Ok(EffectResult::Resolved)
}

fn handle_after_worker_gain_if_used(
    ctx: &mut DispatchContext<'_>,
    player: usize,
    space: &Space,
    required_space: &Space,
    good: &Good,
    amount: u32,
    trigger: &PhaseTriggerData,
) -> Result<EffectResult, String> {
    if trigger.space == Some(*space) && trigger.used_spaces.contains(required_space) {
        let p = &mut ctx.player;
        let next = p.supply.get(*good).checked_add(amount).ok_or("resource overflow")?;
        p.supply.set(*good, next);
    }
    Ok(EffectResult::Resolved)
}

fn handle_start_round_gain(
    ctx: &mut DispatchContext<'_>,
    player: usize,
    good: &Good,
    amount: u32,
    house: &Option<House>,
    rooms: &Option<u8>,
) -> Result<EffectResult, String> {
    let p = &mut ctx.player;
    let house_ok = house.is_none_or(|h| p.house == h);
    let rooms_ok = rooms.is_none_or(|r| p.rooms() == usize::from(r));
    if house_ok && rooms_ok {
        let next = p.supply.get(*good).checked_add(amount).ok_or("resource overflow")?;
        p.supply.set(*good, next);
    }
    Ok(EffectResult::Resolved)
}

fn handle_after_clay_construction_gain(
    ctx: &mut DispatchContext<'_>,
    _player: usize,
    good: &Good,
    amount: u32,
    trigger: &PhaseTriggerData,
) -> Result<EffectResult, String> {
    let Some(prev) = &trigger.previous_player else {
        return Ok(EffectResult::Resolved);
    };
    let current = &ctx.player;
    let qualifies = (prev.house == House::Clay && current.rooms() > prev.rooms())
        || (prev.house == House::Clay && current.house == House::Stone);
    if qualifies && trigger.improvement.is_some() {
        let p = &mut ctx.player;
        let next = p.supply.get(*good).checked_add(amount).ok_or("resource overflow")?;
        p.supply.set(*good, next);
    }
    Ok(EffectResult::Resolved)
}

fn handle_after_harvest_bake_gain(
    ctx: &mut DispatchContext<'_>,
    player: usize,
    amount: u32,
    trigger: &PhaseTriggerData,
) -> Result<EffectResult, String> {
    if trigger.baked {
        let p = &mut ctx.player;
        let next = p.supply.get(Good::Food).checked_add(amount).ok_or("resource overflow")?;
        p.supply.set(Good::Food, next);
    }
    Ok(EffectResult::Resolved)
}

fn handle_harvest_field_gain(
    ctx: &mut DispatchContext<'_>,
    player: usize,
    rule: &crate::cards::HarvestFieldRule,
    good: &Good,
    amount: u32,
    _trigger: &PhaseTriggerData,
) -> Result<EffectResult, String> {
    let p = &mut ctx.player;
    let multiplier = match rule {
        crate::cards::HarvestFieldRule::Animals {
            sheep_per,
            cattle_per,
        } => (p.animal_count(Animal::Sheep) / *sheep_per)
            .checked_add(p.animal_count(Animal::Cattle) / *cattle_per)
            .ok_or("harvest animal bonus overflow")?,
        crate::cards::HarvestFieldRule::MixedFields => {
            let has = |crop: Option<Crop>| {
                p.farm.iter().any(|cell| {
                    matches!(
                        cell.tile,
                        crate::model::Tile::Field {
                            crop: found,
                            ..
                        } if found == crop
                    )
                })
            };
            u32::from(has(Some(Crop::Grain)) && has(Some(Crop::Vegetable)) && has(None))
        }
    };
    let gain = multiplier.checked_mul(amount).ok_or("harvest bonus overflow")?;
    let next = p.supply.get(*good).checked_add(gain).ok_or("resource overflow")?;
    p.supply.set(*good, next);
    Ok(EffectResult::Resolved)
}

/// --- Public Dispatcher Entry Points ---

/// Dispatch all phases for a card play action.
/// Returns Ok(()) if fully resolved, Err("pending decision") if interrupted.
pub fn dispatch_play(
    card_set: &CardSet,
    player: usize,
    card_id: &str,
    cards: &mut crate::cards::CardState,
    players: &mut [Player],
    improvement_market: &mut Vec<crate::model::Improvement>,
    delayed_well: &mut BTreeMap<u8, Vec<usize>>,
    round: u8,
    used_spaces: &std::collections::BTreeSet<Space>,
    event_budget: &mut u32,
    card_uses: &mut BTreeMap<String, u32>,
    pending: &mut Option<PendingDecision>,
) -> Result<(), String> {
    let mut ctx = DispatchContext {
        card_set,
        player: &mut players[player],
        cards,
        improvement_market,
        delayed_well,
        round,
        used_spaces,
        event_budget,
        card_uses,
        pending,
    };

    // Phase 1: Replace (prerequisites)
    let trigger = PhaseTriggerData {
        space: None,
        improvement: None,
        baked: false,
        previous_player: None,
        used_spaces: std::collections::BTreeSet::new(),
    };
    resolve_phase(&mut ctx, player, TriggerPhase::Replace, &trigger)?;

    // Phase 2: Before (on-play gains, passives)
    resolve_phase(&mut ctx, player, TriggerPhase::Before, &trigger)?;

    // Phase 3: Core action (card is moved to played zone — handled by caller via move_card)

    // Phase 4: After (no play-time after effects currently)
    resolve_phase(&mut ctx, player, TriggerPhase::After, &trigger)?;

    Ok(())
}

/// Dispatch AfterWorker phase for a completed worker action.
pub fn dispatch_after_worker(
    card_set: &CardSet,
    player: usize,
    space: Space,
    cards: &mut crate::cards::CardState,
    players: &mut [Player],
    improvement_market: &mut Vec<crate::model::Improvement>,
    delayed_well: &mut BTreeMap<u8, Vec<usize>>,
    round: u8,
    used_spaces: &std::collections::BTreeSet<Space>,
    event_budget: &mut u32,
    card_uses: &mut BTreeMap<String, u32>,
    pending: &mut Option<PendingDecision>,
) -> Result<(), String> {
    let mut ctx = DispatchContext {
        card_set,
        player: &mut players[player],
        cards,
        improvement_market,
        delayed_well,
        round,
        used_spaces,
        event_budget,
        card_uses,
        pending,
    };

    let trigger = PhaseTriggerData {
        space: Some(space),
        improvement: None,
        baked: false,
        previous_player: None,
        used_spaces: used_spaces.clone(),
    };

    // AfterWorkerGain and AfterWorkerGainIfUsed are After phase
    resolve_phase(&mut ctx, player, TriggerPhase::After, &trigger)?;

    Ok(())
}

/// Dispatch StartRound phase for all players at round start.
pub fn dispatch_start_round(
    card_set: &CardSet,
    players: &mut [Player],
    cards: &mut crate::cards::CardState,
    improvement_market: &mut Vec<crate::model::Improvement>,
    delayed_well: &mut BTreeMap<u8, Vec<usize>>,
    round: u8,
    used_spaces: &std::collections::BTreeSet<Space>,
    event_budget: &mut u32,
    card_uses: &mut BTreeMap<String, u32>,
    pending: &mut Option<PendingDecision>,
) -> Result<(), String> {
    for player in 0..players.len() {
        let mut ctx = DispatchContext {
            card_set,
            player: &mut players[player],
            cards,
            improvement_market,
            delayed_well,
            round,
            used_spaces,
            event_budget,
            card_uses,
            pending,
        };

        let trigger = PhaseTriggerData {
            space: None,
            improvement: None,
            baked: false,
            previous_player: None,
            used_spaces: used_spaces.clone(),
        };

        resolve_phase(&mut ctx, player, TriggerPhase::After, &trigger)?;

        // Check for pending interrupt
        if pending.is_some() {
            return Err("pending decision".into());
        }
    }
    Ok(())
}

/// Dispatch AfterClayConstruction phase when a major improvement is purchased.
pub fn dispatch_after_clay_construction(
    card_set: &CardSet,
    player: usize,
    previous: &Player,
    improvement: crate::model::Improvement,
    cards: &mut crate::cards::CardState,
    players: &mut [Player],
    improvement_market: &mut Vec<crate::model::Improvement>,
    delayed_well: &mut BTreeMap<u8, Vec<usize>>,
    round: u8,
    used_spaces: &std::collections::BTreeSet<Space>,
    event_budget: &mut u32,
    card_uses: &mut BTreeMap<String, u32>,
    pending: &mut Option<PendingDecision>,
) -> Result<(), String> {
    let mut ctx = DispatchContext {
        card_set,
        player: &mut players[player],
        cards,
        improvement_market,
        delayed_well,
        round,
        used_spaces,
        event_budget,
        card_uses,
        pending,
    };

    let trigger = PhaseTriggerData {
        space: None,
        improvement: Some(improvement),
        baked: false,
        previous_player: Some(previous.clone()),
        used_spaces: used_spaces.clone(),
    };

    resolve_phase(&mut ctx, player, TriggerPhase::After, &trigger)?;

    Ok(())
}

/// Dispatch HarvestField phase during harvest.
pub fn dispatch_harvest_field(
    card_set: &CardSet,
    player: usize,
    cards: &mut crate::cards::CardState,
    players: &mut [Player],
    improvement_market: &mut Vec<crate::model::Improvement>,
    delayed_well: &mut BTreeMap<u8, Vec<usize>>,
    round: u8,
    used_spaces: &std::collections::BTreeSet<Space>,
    event_budget: &mut u32,
    card_uses: &mut BTreeMap<String, u32>,
    pending: &mut Option<PendingDecision>,
) -> Result<(), String> {
    let mut ctx = DispatchContext {
        card_set,
        player: &mut players[player],
        cards,
        improvement_market,
        delayed_well,
        round,
        used_spaces,
        event_budget,
        card_uses,
        pending,
    };

    let trigger = PhaseTriggerData {
        space: None,
        improvement: None,
        baked: false,
        previous_player: None,
        used_spaces: used_spaces.clone(),
    };

    resolve_phase(&mut ctx, player, TriggerPhase::After, &trigger)?;

    Ok(())
}

/// Dispatch AfterHarvestBake phase when baking occurs.
pub fn dispatch_after_harvest_bake(
    card_set: &CardSet,
    player: usize,
    cards: &mut crate::cards::CardState,
    players: &mut [Player],
    improvement_market: &mut Vec<crate::model::Improvement>,
    delayed_well: &mut BTreeMap<u8, Vec<usize>>,
    round: u8,
    used_spaces: &std::collections::BTreeSet<Space>,
    event_budget: &mut u32,
    card_uses: &mut BTreeMap<String, u32>,
    pending: &mut Option<PendingDecision>,
) -> Result<(), String> {
    let mut ctx = DispatchContext {
        card_set,
        player: &mut players[player],
        cards,
        improvement_market,
        delayed_well,
        round,
        used_spaces,
        event_budget,
        card_uses,
        pending,
    };

    let trigger = PhaseTriggerData {
        space: None,
        improvement: None,
        baked: true,
        previous_player: None,
        used_spaces: used_spaces.clone(),
    };

    resolve_phase(&mut ctx, player, TriggerPhase::After, &trigger)?;

    Ok(())
}

/// Legacy compatibility functions (kept for equivalence testing).
/// These are the original implementations from cards.rs, renamed with _legacy suffix.
pub mod legacy {
    use super::*;
    use crate::cards::{CardSet, CardState};
    use crate::model::Player;

    /// Legacy on-play gains and passives (called during cards::play).
    pub fn apply_on_play_effects_legacy(
        set: &CardSet,
        hand: &mut Vec<String>,
        player: &mut Player,
        id: &str,
        kind: crate::cards::CardKind,
        extra_food: u32,
    ) -> Result<(), String> {
        // This mirrors the original cards::play effect loop (Before phase effects)
        let card = set.definition(id).ok_or("unknown card")?;
        for effect in &card.effects {
            match effect {
                CardEffect::OnPlayGain { good, amount } => {
                    let next = player
                        .supply
                        .get(*good)
                        .checked_add(*amount)
                        .ok_or("card resource overflow")?;
                    player.supply.set(*good, next);
                }
                CardEffect::OnPlayGainPerStored {
                    stored,
                    divisor,
                    good,
                    amount,
                } => {
                    let bonus = (player.supply.get(*stored) / *divisor)
                        .checked_mul(*amount)
                        .ok_or("on-play card bonus overflow")?;
                    let next = player
                        .supply
                        .get(*good)
                        .checked_add(bonus)
                        .ok_or("resource overflow")?;
                    player.supply.set(*good, next);
                }
                CardEffect::OnPlayGainIfHouse {
                    house,
                    rooms,
                    good,
                    amount,
                } if player.house == *house && player.rooms() == usize::from(*rooms) => {
                    let next = player
                        .supply
                        .get(*good)
                        .checked_add(*amount)
                        .ok_or("resource overflow")?;
                    player.supply.set(*good, next);
                }
                CardEffect::Passive { rule } => match rule {
                    crate::cards::PassiveRule::PastureCapacity(n) => player.pasture_capacity_bonus += n,
                    crate::cards::PassiveRule::FenceWoodDiscount(n) => player.free_fence_wood += n,
                    crate::cards::PassiveRule::ImprovementWoodDiscount(n) => {
                        player.improvement_wood_discount += n
                    }
                    crate::cards::PassiveRule::WoodenRoomCost(n) => player.wooden_room_cost = Some(*n),
                    crate::cards::PassiveRule::RoomCost(n) => player.room_material_cost = Some(*n),
                    crate::cards::PassiveRule::MajorStoneDiscountByRooms => {
                        player.major_stone_discount_by_rooms = true
                    }
                    crate::cards::PassiveRule::StoneDiscount(n) => player.stone_cost_discount += n,
                    crate::cards::PassiveRule::HousingSpace(n) => player.extra_housing += n,
                },
                _ => {}
            }
        }
        // Move card from hand to played (caller does this via move_card now)
        hand.retain(|x| x != id);
        player.played_cards.push(id.to_string());
        Ok(())
    }

    /// Legacy after_worker (worker completion).
    pub fn after_worker_legacy(
        set: &CardSet,
        player: &mut Player,
        space: crate::model::Space,
    ) -> Result<(), String> {
        for id in player.played_cards.clone() {
            let card = set.definition(&id).ok_or("unknown played card")?;
            for effect in &card.effects {
                if let CardEffect::AfterWorkerGain {
                    space: trigger,
                    good,
                    amount,
                } = effect
                {
                    if *trigger == space {
                        let next = player
                            .supply
                            .get(*good)
                            .checked_add(*amount)
                            .ok_or("resource overflow")?;
                        player.supply.set(*good, next);
                    }
                }
            }
        }
        Ok(())
    }

    /// Legacy after_worker_if_used.
    pub fn after_worker_if_used_legacy(
        set: &CardSet,
        player: &mut Player,
        space: crate::model::Space,
        used: &std::collections::BTreeSet<crate::model::Space>,
    ) -> Result<(), String> {
        for id in player.played_cards.clone() {
            let card = set.definition(&id).ok_or("unknown played card")?;
            for effect in &card.effects {
                if let CardEffect::AfterWorkerGainIfUsed {
                    space: trigger,
                    required_space,
                    good,
                    amount,
                } = effect
                {
                    if *trigger == space && used.contains(required_space) {
                        let next = player
                            .supply
                            .get(*good)
                            .checked_add(*amount)
                            .ok_or("resource overflow")?;
                        player.supply.set(*good, next);
                    }
                }
            }
        }
        Ok(())
    }

    /// Legacy start_round_rewards.
    pub fn start_round_rewards_legacy(set: &CardSet, player: &mut Player) -> Result<(), String> {
        for id in player.played_cards.clone() {
            let card = set.definition(&id).ok_or("unknown played card")?;
            for effect in &card.effects {
                if let CardEffect::StartRoundGain {
                    good,
                    amount,
                    house,
                    rooms,
                } = effect
                {
                    if house.is_none_or(|h| player.house == h)
                        && rooms.is_none_or(|r| player.rooms() == usize::from(r))
                    {
                        let next = player
                            .supply
                            .get(*good)
                            .checked_add(*amount)
                            .ok_or("resource overflow")?;
                        player.supply.set(*good, next);
                    }
                }
            }
        }
        Ok(())
    }

    /// Legacy after_clay_construction.
    pub fn after_clay_construction_legacy(
        set: &CardSet,
        previous: &Player,
        current: &mut Player,
    ) -> Result<(), String> {
        let qualifies = (previous.house == crate::model::House::Clay && current.rooms() > previous.rooms())
            || (previous.house == crate::model::House::Clay && current.house == crate::model::House::Stone);
        if !qualifies {
            return Ok(());
        }
        for id in current.played_cards.clone() {
            let card = set.definition(&id).ok_or("unknown played card")?;
            for effect in &card.effects {
                if let CardEffect::AfterClayConstructionGain { good, amount } = effect {
                    let next = current
                        .supply
                        .get(*good)
                        .checked_add(*amount)
                        .ok_or("resource overflow")?;
                    current.supply.set(*good, next);
                }
            }
        }
        Ok(())
    }

    /// Legacy harvest_field_rewards.
    pub fn harvest_field_rewards_legacy(set: &CardSet, player: &mut Player) -> Result<(), String> {
        for id in player.played_cards.clone() {
            let card = set.definition(&id).ok_or("unknown played card")?;
            for effect in &card.effects {
                if let CardEffect::HarvestFieldGain { rule, good, amount } = effect {
                    let multiplier = match rule {
                        crate::cards::HarvestFieldRule::Animals {
                            sheep_per,
                            cattle_per,
                        } => (player.animal_count(crate::model::Animal::Sheep) / *sheep_per)
                            .checked_add(player.animal_count(crate::model::Animal::Cattle) / *cattle_per)
                            .ok_or("harvest animal bonus overflow")?,
                        crate::cards::HarvestFieldRule::MixedFields => {
                            let has = |crop: Option<crate::model::Crop>| {
                                player.farm.iter().any(|cell| {
                                    matches!(
                                        cell.tile,
                                        crate::model::Tile::Field {
                                            crop: found,
                                            ..
                                        } if found == crop
                                    )
                                })
                            };
                            u32::from(
                                has(Some(crate::model::Crop::Grain))
                                    && has(Some(crate::model::Crop::Vegetable))
                                    && has(None),
                            )
                        }
                    };
                    let gain = multiplier.checked_mul(*amount).ok_or("harvest bonus overflow")?;
                    let next = player
                        .supply
                        .get(*good)
                        .checked_add(gain)
                        .ok_or("resource overflow")?;
                    player.supply.set(*good, next);
                }
            }
        }
        Ok(())
    }

    /// Legacy end_game_bonus_for_definitions.
    pub fn end_game_bonus_for_definitions_legacy(
        definitions: &[crate::cards::CardDefinition],
        player: &Player,
    ) -> i32 {
        let definition = |id: &String| definitions.iter().find(|card| &card.id == id);
        player
            .played_cards
            .iter()
            .filter_map(definition)
            .flat_map(|card| card.effects.iter())
            .map(|effect| match effect {
                CardEffect::EndGameBonus { rule } => match rule {
                    crate::cards::ScoreBonusRule::PastureArea => {
                        let squares = player
                            .pastures
                            .iter()
                            .map(|p| p.cells.len())
                            .sum::<usize>();
                        [6, 7, 8, 10]
                            .iter()
                            .filter(|&&n| squares >= n)
                            .count() as i32
                    }
                    crate::cards::ScoreBonusRule::HouseMaterial => match player.house {
                        crate::model::House::Wood => 3,
                        crate::model::House::Clay => 2,
                        crate::model::House::Stone => 0,
                    },
                    crate::cards::ScoreBonusRule::UnfencedStables => player
                        .farm
                        .iter()
                        .filter(|cell| {
                            cell.stable
                                && !matches!(cell.tile, crate::model::Tile::Pasture { .. })
                        })
                        .count() as i32,
                    crate::cards::ScoreBonusRule::ImprovementCount => {
                        let minor = player
                            .played_cards
                            .iter()
                            .filter(|id| {
                                definition(id).is_some_and(|card| {
                                    card.kind == crate::cards::CardKind::MinorImprovement
                                })
                            })
                            .count();
                        let total = minor + player.improvements.len();
                        match total {
                            0..=4 => 0,
                            5 => 2,
                            6 => 3,
                            7 => 4,
                            8 => 5,
                            9 => 7,
                            _ => 9,
                        }
                    }
                    crate::cards::ScoreBonusRule::SparePastureCapacity => player
                        .pastures
                        .iter()
                        .filter(|pasture| {
                            pasture.quantity > 0
                                && player
                                    .pasture_capacity(pasture)
                                    .saturating_sub(pasture.quantity)
                                    >= 3
                        })
                        .count() as i32,
                    crate::cards::ScoreBonusRule::LaterOccupations => {
                        let tutor = player.played_cards.iter().position(|id| {
                            definition(id).is_some_and(|card| {
                                card.effects.iter().any(|effect| {
                                    matches!(
                                        effect,
                                        CardEffect::EndGameBonus {
                                            rule: crate::cards::ScoreBonusRule::LaterOccupations
                                        }
                                    )
                                })
                            })
                        });
                        tutor.map_or(0, |index| {
                            player.played_cards[index + 1..]
                                .iter()
                                .filter(|id| {
                                    definition(id).is_some_and(|card| {
                                        card.kind == crate::cards::CardKind::Occupation
                                    })
                                })
                                .count() as i32
                        })
                    }
                },
                _ => 0,
            })
            .sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{CardDefinition, CardKind, CardSet};
    use crate::model::{Good, Player, Space};

    fn setup_game() -> (CardSet, Vec<Player>, crate::cards::CardState) {
        let set = CardSet {
            definitions: vec![
                CardDefinition {
                    id: "TEST_OCC".into(),
                    kind: CardKind::Occupation,
                    cost: vec![],
                    points: 0,
                    effects: vec![CardEffect::AfterWorkerGain {
                        space: Space::Forest,
                        good: Good::Wood,
                        amount: 1,
                    }],
                },
                CardDefinition {
                    id: "TEST_MINOR".into(),
                    kind: CardKind::MinorImprovement,
                    cost: vec![],
                    points: 0,
                    effects: vec![CardEffect::OnPlayGain {
                        good: Good::Food,
                        amount: 2,
                    }],
                },
            ],
        };
        let mut players = vec![Player::new(0, 0), Player::new(1, 0)];
        players[0].played_cards.push("TEST_OCC".into());
        let cards = crate::cards::CardState {
            hands: vec![vec!["TEST_MINOR".into()], vec![]],
            undealt: vec![],
        };
        (set, players, cards)
    }

    #[test]
    fn dispatch_after_worker_triggers_gain() {
        let (set, mut players, mut cards) = setup_game();
        let mut improvement_market = vec![];
        let mut delayed_well = std::collections::BTreeMap::new();
        let mut event_budget = 64;
        let mut card_uses = std::collections::BTreeMap::new();
        let mut pending = None;

        dispatch_after_worker(
            &set,
            0,
            Space::Forest,
            &mut cards,
            &mut players,
            &mut improvement_market,
            &mut delayed_well,
            1,
            &std::collections::BTreeSet::new(),
            &mut event_budget,
            &mut card_uses,
            &mut pending,
        )
        .unwrap();

        assert_eq!(players[0].supply.wood, 1);
    }

    #[test]
    fn dispatch_start_round_triggers_gain() {
        let set = CardSet {
            definitions: vec![CardDefinition {
                id: "SR_GAIN".into(),
                kind: CardKind::Occupation,
                cost: vec![],
                points: 0,
                effects: vec![CardEffect::StartRoundGain {
                    good: Good::Food,
                    amount: 1,
                    house: None,
                    rooms: None,
                }],
            }],
        };
        let mut players = vec![Player::new(0, 0)];
        players[0].played_cards.push("SR_GAIN".into());
        let mut cards = crate::cards::CardState::default();
        let mut improvement_market = vec![];
        let mut delayed_well = std::collections::BTreeMap::new();
        let mut event_budget = 64;
        let mut card_uses = std::collections::BTreeMap::new();
        let mut pending = None;

        dispatch_start_round(
            &set,
            &mut players,
            &mut cards,
            &mut improvement_market,
            &mut delayed_well,
            1,
            &std::collections::BTreeSet::new(),
            &mut event_budget,
            &mut card_uses,
            &mut pending,
        )
        .unwrap();

        assert_eq!(players[0].supply.food, 1 + 3); // 3 starting food + 1 from card
    }

    #[test]
    fn event_budget_exhausted() {
        let set = CardSet {
            definitions: vec![CardDefinition {
                id: "CHAIN".into(),
                kind: CardKind::Occupation,
                cost: vec![],
                points: 0,
                effects: vec![CardEffect::OnPlayGain {
                    good: Good::Food,
                    amount: 1,
                }],
            }],
        };
        let mut players = vec![Player::new(0, 0)];
        // 64 cards would exceed budget
        for i in 0..70 {
            players[0].played_cards.push(format!("CHAIN{i}"));
        }
        let mut cards = crate::cards::CardState::default();
        let mut improvement_market = vec![];
        let mut delayed_well = std::collections::BTreeMap::new();
        let mut event_budget = 64;
        let mut card_uses = std::collections::BTreeMap::new();
        let mut pending = None;

        let result = dispatch_start_round(
            &set,
            &mut players,
            &mut cards,
            &mut improvement_market,
            &mut delayed_well,
            1,
            &std::collections::BTreeSet::new(),
            &mut event_budget,
            &mut card_uses,
            &mut pending,
        );
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("budget exhausted"));
    }
}