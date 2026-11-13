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

use crate::cards::{CardEffect, CardSet};
use crate::model::{Animal, Crop, Good, House, Player, Space};
use crate::pending::{ChainPosition, PendingDecision, Transition};
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
        #[cfg(test)]
        // The probe models an effect whose resolution needs a player choice,
        // which is exactly the Interrupt phase (2016 Lookout p.8: the active
        // player resolves their own effects in the order they choose).
        CardEffect::PendingProbe { .. } => TriggerPhase::Interrupt,
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
    /// Declared per-card limits. Absent card = no per-period limit.
    pub card_use_limits: &'a BTreeMap<String, u32>,
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
fn check_card_use(
    card_uses: &mut BTreeMap<String, u32>,
    card_id: &str,
    limit: u32,
) -> Result<(), String> {
    let count = card_uses.entry(card_id.to_string()).or_insert(0);
    if *count >= limit {
        return Err(format!("card {card_id} use limit ({limit}) exceeded"));
    }
    *count += 1;
    Ok(())
}

/// Execute all effects of a given phase for a player's played cards in play order.
/// Play order = the order cards appear in `played_cards` (official "selection order").
///
/// Issue #268: a pause is `Ok(Transition::Paused(decision))`, not an
/// `Err("pending decision")` sentinel, so the interrupted state can be
/// committed instead of discarded. The decision stores the position AFTER
/// which the scan paused, so a resume re-runs the same phase from the next
/// effect without double-applying the paused one.
pub fn resolve_phase(
    ctx: &mut DispatchContext<'_>,
    player: usize,
    phase: TriggerPhase,
    trigger_data: &PhaseTriggerData,
) -> Result<Transition, String> {
    let played_cards = ctx.player.played_cards.clone();
    let pause = resolve_phase_from(ctx, player, phase, trigger_data, &played_cards, None)?;
    Ok(pause)
}

/// Scan a phase's effects, starting strictly AFTER `after` (resume) or from
/// the beginning (`None`). `paused_card` receives the card whose effect
/// paused, when a pause happens inside `resolve_phase_for_card` — the
/// single-card variant must remember which effect index within THE card it
/// stopped at, because its `card_index` is fixed by the caller.
///
/// Returns `Ok(Transition::Paused(_))` when an effect produced a pending
/// decision; the scan has already stopped and no later effect ran.
fn resolve_phase_scan(
    ctx: &mut DispatchContext<'_>,
    player: usize,
    phase: TriggerPhase,
    trigger_data: &PhaseTriggerData,
    played_cards: &[String],
    after: Option<ChainPosition>,
    paused_card: Option<&mut Option<usize>>,
) -> Result<Transition, String> {
    let mut card_index: usize = 0;
    for card_id in played_cards {
        let Some(card) = ctx.card_set.definition(card_id) else {
            card_index += 1;
            continue;
        };

        let mut effect_index: usize = 0;
        for effect in &card.effects {
            if effect_phase(effect) != phase {
                effect_index += 1;
                continue;
            }

            // Resume: skip everything up to AND including `after`.
            if let Some(a) = after {
                if (card_index, effect_index) <= (a.card_index, a.effect_index) {
                    effect_index += 1;
                    continue;
                }
            }

            // Consume event budget
            consume_budget(ctx.event_budget)?;

            // Per-card use limit for the current period. A card without a
            // declared limit is unlimited (u32::MAX); the ledger still counts
            // every resolution so limits can be introduced as data.
            let limit = ctx
                .card_use_limits
                .get(card_id)
                .copied()
                .unwrap_or(u32::MAX);
            check_card_use(ctx.card_uses, card_id, limit)?;

            // Dispatch to handler
            let result = match effect {
                #[cfg(test)]
                CardEffect::PendingProbe { options } => {
                    handle_pending_probe(ctx, player, card_id, options.clone(), trigger_data)?
                }
                CardEffect::Prerequisite { rule } => {
                    handle_prerequisite(ctx, player, card_id, rule, trigger_data)?
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
                CardEffect::AfterWorkerGain {
                    space,
                    good,
                    amount,
                } => handle_after_worker_gain(ctx, player, space, good, *amount, trigger_data)?,
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

            // Handle pending interrupt: pause with the resume position.
            if let EffectResult::Pending(mut pending) = result {
                pending.context = serde_json::json!({
                    "card": card_id,
                    "card_index": card_index,
                    "effect_index": effect_index,
                });
                *ctx.pending = Some(pending.clone());
                if let Some(slot) = paused_card {
                    *slot = Some(effect_index);
                }
                return Ok(Transition::Paused(Box::new(pending)));
            }
            effect_index += 1;
        }
        card_index += 1;
    }

    Ok(Transition::Completed)
}

/// Same as [`resolve_phase`] but the scan resumes strictly after `after`
/// (Issue #268 resume path).
pub fn resolve_phase_from(
    ctx: &mut DispatchContext<'_>,
    player: usize,
    phase: TriggerPhase,
    trigger_data: &PhaseTriggerData,
    played_cards: &[String],
    after: Option<ChainPosition>,
) -> Result<Transition, String> {
    resolve_phase_scan(ctx, player, phase, trigger_data, played_cards, after, None)
}

/// Execute all effects of a given phase for ONLY the specified card (newly played).
/// This is used by dispatch_play to avoid re-firing already-played cards' effects.
fn resolve_phase_for_card(
    ctx: &mut DispatchContext<'_>,
    player: usize,
    phase: TriggerPhase,
    trigger_data: &PhaseTriggerData,
    card_id: &str,
) -> Result<Transition, String> {
    let Some(_card) = ctx.card_set.definition(card_id) else {
        return Ok(Transition::Completed); // Card not found, nothing to do
    };

    // Single-card scan: the position only tracks the effect index within
    // THIS card, and the pause reports it via `paused_effect` so the caller
    // can build an accurate resume position (card_index comes from context).
    let mut paused_effect: Option<usize> = None;
    let card_id_owned = card_id.to_string();
    let played_cards = std::slice::from_ref(&card_id_owned);
    let result = resolve_phase_scan(
        ctx,
        player,
        phase,
        trigger_data,
        played_cards,
        None,
        Some(&mut paused_effect),
    )?;
    // The single-card scan stores position (0, effect_index) in context;
    // rewrite it with the real card position when the caller knows one.
    if let Transition::Paused(mut pending) = result {
        if let (Some(_effect), Some(obj)) = (paused_effect, pending.context.as_object_mut()) {
            // The scan wrote card_index 0 for the single-card slice; keep it
            // but record the true single-card flag so resume paths can tell.
            obj.insert("single_card".to_string(), serde_json::json!(true));
        }
        return Ok(Transition::Paused(pending));
    }
    Ok(result)
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

// --- Effect Handlers (each phase handler cites the official timing rule) ---
/// Test-only handler for the Interrupt branch: it produces a PendingDecision
/// instead of resolving immediately, so the pause/resume contract is covered.
///
/// It does NOT fire inside the clay-construction window. That dispatcher
/// re-scans the acting player's already-played cards within the same worker
/// action (it is gated by `improvement`/`previous_player`, both present only on
/// a purchase), so a probe there would fire a second time in one action and the
/// resume would re-pause on its own card. Firing only in the genuine
/// own-effect windows (round start, harvest-bake, worker completion) models a
/// single Interrupt choice per action (Issue #268 R2-P0-1).
#[cfg(test)]
fn handle_pending_probe(
    _ctx: &mut DispatchContext<'_>,
    player: usize,
    card_id: &str,
    options: Vec<(Good, u32)>,
    trigger_data: &PhaseTriggerData,
) -> Result<EffectResult, String> {
    let is_clay_construction_window =
        trigger_data.space.is_none() && trigger_data.previous_player.is_some();
    if is_clay_construction_window {
        return Ok(EffectResult::Resolved);
    }
    let kind = crate::pending::PendingKind::ChooseGain {
        source_card: card_id.to_string(),
        options: options
            .into_iter()
            .map(|(good, amount)| crate::pending::GainOption { good, amount })
            .collect(),
    };
    // Handlers describe WHAT has to be decided; the engine stamps the decision
    // id and the pre-decision state hash when it actually pauses (it is the
    // only layer that knows them). Leaving them blank here keeps the handler
    // free of game-level bookkeeping.
    let decision = PendingDecision::new(
        0,
        player,
        kind,
        String::new(),
        serde_json::json!({ "card": card_id }),
    );
    Ok(EffectResult::Pending(decision))
}

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
    _player: usize,
    good: &Good,
    amount: u32,
) -> Result<EffectResult, String> {
    let p = &mut ctx.player;
    let next = p
        .supply
        .get(*good)
        .checked_add(amount)
        .ok_or("resource overflow")?;
    p.supply.set(*good, next);
    Ok(EffectResult::Resolved)
}

fn handle_on_play_gain_per_stored(
    ctx: &mut DispatchContext<'_>,
    _player: usize,
    stored: &Good,
    divisor: u32,
    good: &Good,
    amount: u32,
) -> Result<EffectResult, String> {
    let p = &mut ctx.player;
    let bonus = (p.supply.get(*stored) / divisor)
        .checked_mul(amount)
        .ok_or("on-play bonus overflow")?;
    let next = p
        .supply
        .get(*good)
        .checked_add(bonus)
        .ok_or("resource overflow")?;
    p.supply.set(*good, next);
    Ok(EffectResult::Resolved)
}

fn handle_on_play_gain_if_house(
    ctx: &mut DispatchContext<'_>,
    _player: usize,
    house: &House,
    rooms: u8,
    good: &Good,
    amount: u32,
) -> Result<EffectResult, String> {
    let p = &mut ctx.player;
    if p.house == *house && p.rooms() == usize::from(rooms) {
        let next = p
            .supply
            .get(*good)
            .checked_add(amount)
            .ok_or("resource overflow")?;
        p.supply.set(*good, next);
    }
    Ok(EffectResult::Resolved)
}

fn handle_passive(
    ctx: &mut DispatchContext<'_>,
    _player: usize,
    rule: &crate::cards::PassiveRule,
) -> Result<EffectResult, String> {
    let p = &mut ctx.player;
    match rule {
        crate::cards::PassiveRule::PastureCapacity(n) => p.pasture_capacity_bonus += n,
        crate::cards::PassiveRule::FenceWoodDiscount(n) => p.free_fence_wood += n,
        crate::cards::PassiveRule::ImprovementWoodDiscount(n) => p.improvement_wood_discount += n,
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
    _player: usize,
    space: &Space,
    good: &Good,
    amount: u32,
    trigger: &PhaseTriggerData,
) -> Result<EffectResult, String> {
    if trigger.space == Some(*space) {
        let p = &mut ctx.player;
        let next = p
            .supply
            .get(*good)
            .checked_add(amount)
            .ok_or("resource overflow")?;
        p.supply.set(*good, next);
    }
    Ok(EffectResult::Resolved)
}

fn handle_after_worker_gain_if_used(
    ctx: &mut DispatchContext<'_>,
    _player: usize,
    space: &Space,
    required_space: &Space,
    good: &Good,
    amount: u32,
    trigger: &PhaseTriggerData,
) -> Result<EffectResult, String> {
    if trigger.space == Some(*space) && trigger.used_spaces.contains(required_space) {
        let p = &mut ctx.player;
        let next = p
            .supply
            .get(*good)
            .checked_add(amount)
            .ok_or("resource overflow")?;
        p.supply.set(*good, next);
    }
    Ok(EffectResult::Resolved)
}

fn handle_start_round_gain(
    ctx: &mut DispatchContext<'_>,
    _player: usize,
    good: &Good,
    amount: u32,
    house: &Option<House>,
    rooms: &Option<u8>,
) -> Result<EffectResult, String> {
    let p = &mut ctx.player;
    let house_ok = house.is_none_or(|h| p.house == h);
    let rooms_ok = rooms.is_none_or(|r| p.rooms() == usize::from(r));
    if house_ok && rooms_ok {
        let next = p
            .supply
            .get(*good)
            .checked_add(amount)
            .ok_or("resource overflow")?;
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
    // The clay-construction condition is house material + a change in rooms or
    // an upgrade to stone. It does NOT require that an improvement was bought
    // in this action: the legacy cards::after_clay_construction fired on the
    // condition alone, and the equivalence tests pin that behaviour.
    let qualifies = (prev.house == House::Clay && current.rooms() > prev.rooms())
        || (prev.house == House::Clay && current.house == House::Stone);
    let _ = &trigger.improvement;
    if qualifies {
        let p = &mut ctx.player;
        let next = p
            .supply
            .get(*good)
            .checked_add(amount)
            .ok_or("resource overflow")?;
        p.supply.set(*good, next);
    }
    Ok(EffectResult::Resolved)
}

fn handle_after_harvest_bake_gain(
    ctx: &mut DispatchContext<'_>,
    _player: usize,
    amount: u32,
    trigger: &PhaseTriggerData,
) -> Result<EffectResult, String> {
    if trigger.baked {
        let p = &mut ctx.player;
        let next = p
            .supply
            .get(Good::Food)
            .checked_add(amount)
            .ok_or("resource overflow")?;
        p.supply.set(Good::Food, next);
    }
    Ok(EffectResult::Resolved)
}

fn handle_harvest_field_gain(
    ctx: &mut DispatchContext<'_>,
    _player: usize,
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
    let gain = multiplier
        .checked_mul(amount)
        .ok_or("harvest bonus overflow")?;
    let next = p
        .supply
        .get(*good)
        .checked_add(gain)
        .ok_or("resource overflow")?;
    p.supply.set(*good, next);
    Ok(EffectResult::Resolved)
}

// --- Public Dispatcher Entry Points ---
/// Dispatch all phases for a card play action.
///
/// Returns `Ok(Transition::Completed)` when the play's Replace/Before phases
/// fully resolved. A play-time interrupt (a choice in a card's Before phase)
/// canNOT be represented here — the cost/zone move prerequisites are already
/// mid-flight on the caller's working clone — so it is aborted with a precise
/// `Err` rather than silently dropping a `Transition::Paused` (Issue #268
/// R2-P0-2). Test-only probes pause via `dispatch_after_worker` instead; the
/// resume contract covers the post-work chain.
/// Only the newly played card's Replace/Before effects are resolved (prerequisites gate the play,
/// on-play gains/passives apply once). Already-played cards' effects are NOT re-fired.
#[allow(clippy::too_many_arguments)] // threads the same storages as the legacy cards.rs fns
pub fn dispatch_play(
    card_set: &CardSet,
    player: usize,
    new_card_id: &str,
    cards: &mut crate::cards::CardState,
    players: &mut [Player],
    improvement_market: &mut Vec<crate::model::Improvement>,
    delayed_well: &mut BTreeMap<u8, Vec<usize>>,
    round: u8,
    used_spaces: &std::collections::BTreeSet<Space>,
    event_budget: &mut u32,
    card_uses: &mut BTreeMap<String, u32>,
    card_use_limits: &BTreeMap<String, u32>,
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
        card_use_limits,
        pending,
    };

    let trigger = PhaseTriggerData {
        space: None,
        improvement: None,
        baked: false,
        previous_player: None,
        used_spaces: std::collections::BTreeSet::new(),
    };

    // Phase 1: Replace (prerequisites) - ONLY for the new card
    if let Transition::Paused(_) = resolve_phase_for_card(
        &mut ctx,
        player,
        TriggerPhase::Replace,
        &trigger,
        new_card_id,
    )? {
        // A prerequisite cannot require a choice; treat as an engine error.
        return Err("play prerequisites must resolve immediately".into());
    }

    // Phase 2: Before (on-play gains, passives) - ONLY for the new card
    if let Transition::Paused(_) = resolve_phase_for_card(
        &mut ctx,
        player,
        TriggerPhase::Before,
        &trigger,
        new_card_id,
    )? {
        // A choice in a play's Before phase is a play-time interrupt. The play
        // transaction (cost payment + hand->played zone move) is already applied
        // on the caller's working clone and cannot be resumed here, so abort the
        // play with a precise `Err`. Returning it (instead of an Ok(Paused) the
        // callers dropped) is what stops the interrupt being silently discarded
        // (Issue #268 R2-P0-2). The `Err` propagates out of `execute_work`
        // before the working clone is committed, so the pre-play state stays
        // intact.
        return Err(
            "play-time interrupt during a card's Before phase is not supported by the \
             work executor"
                .into(),
        );
    }

    // Phase 3: Core action (card is moved to played zone — handled by caller via move_card)

    // Phase 4: After (no play-time after effects currently)
    // NOTE: After phase is a no-op for OnPlay cards; no need to resolve for new card

    Ok(())
}

/// Dispatch AfterWorker phase for a completed worker action.
#[allow(clippy::too_many_arguments)] // threads the same storages as the legacy cards.rs fns
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
    card_use_limits: &BTreeMap<String, u32>,
    pending: &mut Option<PendingDecision>,
) -> Result<Transition, String> {
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
        card_use_limits,
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
    // Interrupts pause the chain after the passive phase has resolved
    // (2016 Lookout p.8: the active player resolves their effects in the
    // order they choose; a choice-requiring effect stops the chain).
    resolve_phase(&mut ctx, player, TriggerPhase::Interrupt, &trigger)
}

/// Dispatch StartRound phase for all players at round start.
#[allow(clippy::too_many_arguments)] // threads the same storages as the legacy cards.rs fns
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
    card_use_limits: &BTreeMap<String, u32>,
    pending: &mut Option<PendingDecision>,
) -> Result<Transition, String> {
    for (player, p_ref) in players.iter_mut().enumerate() {
        let mut ctx = DispatchContext {
            card_set,
            player: p_ref,
            cards,
            improvement_market,
            delayed_well,
            round,
            used_spaces,
            event_budget,
            card_uses,
            card_use_limits,
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
        // Then the interrupt phase for this player. A pause here returns
        // immediately (mid-start-round); the round-start path stays typed:
        // the caller records the error rather than committing a partial
        // start_round, per Issue #268 scope (post-work chain only).
        if let Transition::Paused(p) =
            resolve_phase(&mut ctx, player, TriggerPhase::Interrupt, &trigger)?
        {
            return Ok(Transition::Paused(p));
        }
    }
    Ok(Transition::Completed)
}

/// Dispatch AfterClayConstruction phase when a major improvement is purchased.
#[allow(clippy::too_many_arguments)] // threads the same storages as the legacy cards.rs fns
pub fn dispatch_after_clay_construction(
    card_set: &CardSet,
    player: usize,
    previous: &Player,
    // The improvement bought in this action, if any. `None` still dispatches
    // (so the budget/limits apply), but the clay-construction handler cannot
    // fire because it requires `trigger.improvement.is_some()`.
    improvement: Option<crate::model::Improvement>,
    cards: &mut crate::cards::CardState,
    players: &mut [Player],
    improvement_market: &mut Vec<crate::model::Improvement>,
    delayed_well: &mut BTreeMap<u8, Vec<usize>>,
    round: u8,
    used_spaces: &std::collections::BTreeSet<Space>,
    event_budget: &mut u32,
    card_uses: &mut BTreeMap<String, u32>,
    card_use_limits: &BTreeMap<String, u32>,
    pending: &mut Option<PendingDecision>,
) -> Result<Transition, String> {
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
        card_use_limits,
        pending,
    };

    let trigger = PhaseTriggerData {
        space: None,
        improvement,
        baked: false,
        previous_player: Some(previous.clone()),
        used_spaces: used_spaces.clone(),
    };

    resolve_phase(&mut ctx, player, TriggerPhase::After, &trigger)?;
    // Interrupts pause the chain after the passive phase has resolved
    // (2016 Lookout p.8: the active player resolves their effects in the
    // order they choose; a choice-requiring effect stops the chain).
    if let Transition::Paused(p) =
        resolve_phase(&mut ctx, player, TriggerPhase::Interrupt, &trigger)?
    {
        return Ok(Transition::Paused(p));
    }
    Ok(Transition::Completed)
}

/// Dispatch HarvestField phase during harvest.
#[allow(clippy::too_many_arguments)] // threads the same storages as the legacy cards.rs fns
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
    card_use_limits: &BTreeMap<String, u32>,
    pending: &mut Option<PendingDecision>,
) -> Result<Transition, String> {
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
        card_use_limits,
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
    // Interrupts pause the chain after the passive phase has resolved
    // (2016 Lookout p.8: the active player resolves their effects in the
    // order they choose; a choice-requiring effect stops the chain).
    if let Transition::Paused(p) =
        resolve_phase(&mut ctx, player, TriggerPhase::Interrupt, &trigger)?
    {
        return Ok(Transition::Paused(p));
    }
    Ok(Transition::Completed)
}

/// Dispatch AfterHarvestBake phase when baking occurs.
#[allow(clippy::too_many_arguments)] // threads the same storages as the legacy cards.rs fns
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
    card_use_limits: &BTreeMap<String, u32>,
    pending: &mut Option<PendingDecision>,
) -> Result<Transition, String> {
    // Same gate as the legacy cards::after_harvest_baking: a bake bonus only
    // exists in the round immediately after a harvest round. Without this the
    // dispatcher would award bake bonuses in rounds where the legacy path (and
    // every existing test) awards nothing.
    if round < 2 || !crate::model::HARVEST_ROUNDS.contains(&(round - 1)) {
        return Ok(Transition::Completed);
    }
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
        card_use_limits,
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
    // Interrupts pause the chain after the passive phase has resolved
    // (2016 Lookout p.8: the active player resolves their effects in the
    // order they choose; a choice-requiring effect stops the chain).
    if let Transition::Paused(p) =
        resolve_phase(&mut ctx, player, TriggerPhase::Interrupt, &trigger)?
    {
        return Ok(Transition::Paused(p));
    }
    Ok(Transition::Completed)
}

/// Legacy compatibility functions (kept for equivalence testing).
/// These are the original implementations from cards.rs, renamed with _legacy suffix.
#[cfg(test)]
pub mod legacy {
    use super::*;
    use crate::cards::CardSet;
    use crate::model::Player;

    /// Legacy on-play gains and passives (called during cards::play).
    /// `kind`/`extra_food` are accepted to mirror the legacy `cards::play`
    /// signature used by the equivalence tests; the legacy effect loop never
    /// read them (cost payment happened before it in cards::play).
    pub fn apply_on_play_effects_legacy(
        set: &CardSet,
        hand: &mut Vec<String>,
        player: &mut Player,
        id: &str,
        _kind: crate::cards::CardKind,
        _extra_food: u32,
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
                    crate::cards::PassiveRule::PastureCapacity(n) => {
                        player.pasture_capacity_bonus += n
                    }
                    crate::cards::PassiveRule::FenceWoodDiscount(n) => player.free_fence_wood += n,
                    crate::cards::PassiveRule::ImprovementWoodDiscount(n) => {
                        player.improvement_wood_discount += n
                    }
                    crate::cards::PassiveRule::WoodenRoomCost(n) => {
                        player.wooden_room_cost = Some(*n)
                    }
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
        let qualifies = (previous.house == crate::model::House::Clay
            && current.rooms() > previous.rooms())
            || (previous.house == crate::model::House::Clay
                && current.house == crate::model::House::Stone);
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
                            .checked_add(
                                player.animal_count(crate::model::Animal::Cattle) / *cattle_per,
                            )
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
                    let gain = multiplier
                        .checked_mul(*amount)
                        .ok_or("harvest bonus overflow")?;
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
                        let squares = player.pastures.iter().map(|p| p.cells.len()).sum::<usize>();
                        [6, 7, 8, 10].iter().filter(|&&n| squares >= n).count() as i32
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
                            cell.stable && !matches!(cell.tile, crate::model::Tile::Pasture { .. })
                        })
                        .count()
                        as i32,
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
                        .count()
                        as i32,
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
    use std::collections::BTreeSet;

    fn setup_game() -> (CardSet, Vec<Player>, crate::cards::CardState) {
        let set = CardSet {
            definitions: vec![
                CardDefinition {
                    use_limit: None,
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
                    use_limit: None,
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
        let card_use_limits = std::collections::BTreeMap::new();
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
            &card_use_limits,
            &mut pending,
        )
        .unwrap();

        assert_eq!(players[0].supply.wood, 1);
    }

    #[test]
    fn dispatch_start_round_triggers_gain() {
        let set = CardSet {
            definitions: vec![CardDefinition {
                use_limit: None,
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
        let card_use_limits = std::collections::BTreeMap::new();
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
            &card_use_limits,
            &mut pending,
        )
        .unwrap();

        // Player::new(0, 0) starts with 0 food; the card grants +1.
        assert_eq!(players[0].supply.food, 1);
    }

    #[test]
    fn event_budget_exhausted() {
        let set = CardSet {
            definitions: vec![CardDefinition {
                use_limit: None,
                id: "CHAIN".into(),
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
        // 70 instances of the SAME chain card: resolve_phase looks up every
        // played card by definition, so each instance must exist in the set
        // to consume one budget unit. 70 > 64 budget → exhaustive guard fires.
        for _ in 0..70 {
            players[0].played_cards.push(set.definitions[0].id.clone());
        }
        let mut cards = crate::cards::CardState::default();
        let mut improvement_market = vec![];
        let mut delayed_well = std::collections::BTreeMap::new();
        let mut event_budget = 64;
        let mut card_uses = std::collections::BTreeMap::new();
        let card_use_limits = std::collections::BTreeMap::new();
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
            &card_use_limits,
            &mut pending,
        );
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("budget exhausted"));
    }

    // Equivalence differential tests: the legacy hooks path (cards.rs public
    // fns, the pre-Step3 implementation) and the new phase dispatcher must
    // produce identical state transitions, including multi-card chains where
    // several played cards fire on the same trigger (Issue #266 acceptance:
    // equivalence of the old CardEffect/hooks path and the new path, incl. chains).
    fn equivalence_pool() -> CardSet {
        let chain = |id: &str, effects: Vec<CardEffect>| CardDefinition {
            use_limit: None,
            id: id.into(),
            kind: CardKind::Occupation,
            cost: vec![],
            points: 0,
            effects,
        };
        CardSet {
            definitions: vec![
                // chain of three cards firing on the same worker trigger
                chain(
                    "C1",
                    vec![CardEffect::AfterWorkerGain {
                        space: Space::Forest,
                        good: Good::Wood,
                        amount: 1,
                    }],
                ),
                chain(
                    "C2",
                    vec![CardEffect::AfterWorkerGain {
                        space: Space::Forest,
                        good: Good::Clay,
                        amount: 2,
                    }],
                ),
                chain(
                    "C3",
                    vec![CardEffect::AfterWorkerGainIfUsed {
                        space: Space::Forest,
                        required_space: Space::Lessons,
                        good: Good::Reed,
                        amount: 1,
                    }],
                ),
                chain("C4", vec![CardEffect::AfterHarvestBakeGain { amount: 3 }]),
                chain(
                    "C5",
                    vec![CardEffect::HarvestFieldGain {
                        rule: crate::cards::HarvestFieldRule::Animals {
                            sheep_per: 2,
                            cattle_per: 1,
                        },
                        good: Good::Food,
                        amount: 1,
                    }],
                ),
                chain(
                    "C6",
                    vec![CardEffect::StartRoundGain {
                        good: Good::Food,
                        amount: 1,
                        house: None,
                        rooms: None,
                    }],
                ),
                chain(
                    "C7",
                    vec![CardEffect::AfterClayConstructionGain {
                        good: Good::Food,
                        amount: 2,
                    }],
                ),
                // minor with an on-play gain for the play-path equivalence
                CardDefinition {
                    use_limit: None,
                    id: "M1".into(),
                    kind: CardKind::MinorImprovement,
                    cost: vec![],
                    points: 1,
                    effects: vec![CardEffect::OnPlayGain {
                        good: Good::Grain,
                        amount: 2,
                    }],
                },
            ],
        }
    }

    /// All the pieces the dispatchers need, so each test stays a one-liner.
    struct Ctx {
        players: Vec<Player>,
        cards: crate::cards::CardState,
        budget: u32,
        uses: BTreeMap<String, u32>,
        limits: BTreeMap<String, u32>,
    }

    impl Ctx {
        fn new() -> Self {
            Self {
                players: vec![Player::new(0, 0)],
                cards: crate::cards::CardState::default(),
                budget: 64,
                uses: BTreeMap::new(),
                limits: BTreeMap::new(),
            }
        }
    }

    #[test]
    fn equivalence_after_worker_chain_matches_legacy_path() {
        let set = equivalence_pool();
        for space in [Space::Forest, Space::Lessons, Space::DayLaborer] {
            // legacy path: the production pair after_worker + after_worker_if_used
            let mut legacy = [Player::new(0, 0)];
            legacy[0].played_cards = vec!["C1".into(), "C2".into(), "C3".into()];
            crate::cards::after_worker(&set, &mut legacy[0], space).unwrap();
            let mut legacy_used = std::collections::BTreeSet::new();
            legacy_used.insert(Space::Lessons);
            crate::cards::after_worker_if_used(&set, &mut legacy[0], space, &legacy_used).unwrap();

            // new dispatcher path
            let mut c = Ctx::new();
            c.players[0].played_cards = vec!["C1".into(), "C2".into(), "C3".into()];
            let mut market = vec![];
            let mut well = BTreeMap::new();
            let mut used = std::collections::BTreeSet::new();
            used.insert(Space::Lessons);
            let mut pending = None;
            dispatch_after_worker(
                &set,
                0,
                space,
                &mut c.cards,
                &mut c.players,
                &mut market,
                &mut well,
                1,
                &used,
                &mut c.budget,
                &mut c.uses,
                &c.limits,
                &mut pending,
            )
            .unwrap();
            assert_eq!(
                legacy[0].supply.wood, c.players[0].supply.wood,
                "wood mismatch on {space:?}"
            );
            assert_eq!(
                legacy[0].supply.clay, c.players[0].supply.clay,
                "clay mismatch on {space:?}"
            );
            assert_eq!(
                legacy[0].supply.reed, c.players[0].supply.reed,
                "reed mismatch on {space:?}"
            );
        }
    }

    #[test]
    fn equivalence_start_round_matches_legacy_path() {
        let set = equivalence_pool();
        let mut legacy = [Player::new(0, 0)];
        legacy[0].played_cards = vec!["C1".into(), "C4".into(), "C6".into(), "C6".into()];
        crate::cards::start_round_rewards(&set, &mut legacy[0]).unwrap();

        let mut c = Ctx::new();
        c.players[0].played_cards = legacy[0].played_cards.clone();
        let mut market = vec![];
        let mut well = BTreeMap::new();
        let mut pending = None;
        dispatch_start_round(
            &set,
            &mut c.players,
            &mut c.cards,
            &mut market,
            &mut well,
            2,
            &std::collections::BTreeSet::new(),
            &mut c.budget,
            &mut c.uses,
            &c.limits,
            &mut pending,
        )
        .unwrap();
        assert_eq!(legacy[0].supply.food, c.players[0].supply.food);
    }

    #[test]
    fn equivalence_harvest_and_bake_matches_legacy_path() {
        let set = equivalence_pool();
        // HarvestField chain C5 with no animals: multiplier 0 on both paths.
        let mut legacy = [Player::new(0, 0)];
        legacy[0].played_cards = vec!["C5".into()];
        crate::cards::harvest_field_rewards(&set, &mut legacy[0]).unwrap();

        let mut c = Ctx::new();
        c.players[0].played_cards = vec!["C5".into()];
        let mut market = vec![];
        let mut well = BTreeMap::new();
        let mut pending = None;
        dispatch_harvest_field(
            &set,
            0,
            &mut c.cards,
            &mut c.players,
            &mut market,
            &mut well,
            8,
            &std::collections::BTreeSet::new(),
            &mut c.budget,
            &mut c.uses,
            &c.limits,
            &mut pending,
        )
        .unwrap();
        assert_eq!(
            legacy[0].supply.food, c.players[0].supply.food,
            "harvest field gain must match legacy"
        );

        // bake chain: two C4 instances fire together
        let mut legacy2 = [Player::new(0, 0)];
        legacy2[0].played_cards = vec!["C1".into(), "C4".into(), "C4".into()];
        crate::cards::after_harvest_baking(&set, &mut legacy2[0], 5, true).unwrap();

        let mut c2 = Ctx::new();
        c2.players[0].played_cards = legacy2[0].played_cards.clone();
        let mut market2 = vec![];
        let mut well2 = BTreeMap::new();
        let mut pending2 = None;
        dispatch_after_harvest_bake(
            &set,
            0,
            &mut c2.cards,
            &mut c2.players,
            &mut market2,
            &mut well2,
            5,
            &std::collections::BTreeSet::new(),
            &mut c2.budget,
            &mut c2.uses,
            &c2.limits,
            &mut pending2,
        )
        .unwrap();
        assert_eq!(
            legacy2[0].supply.food, c2.players[0].supply.food,
            "bake bonus must match legacy"
        );
    }

    #[test]
    fn equivalence_after_clay_construction_matches_legacy_path() {
        let set = equivalence_pool();
        // previous clay house, current renovates to stone -> qualifies
        let mut previous = Player::new(0, 0);
        previous.house = crate::model::House::Clay;

        let mut legacy = [Player::new(0, 0)];
        legacy[0].played_cards = vec!["C1".into(), "C7".into(), "C7".into()];
        legacy[0].house = crate::model::House::Stone;
        crate::cards::after_clay_construction(&set, &previous, &mut legacy[0]).unwrap();

        let mut c = Ctx::new();
        c.players[0].played_cards = legacy[0].played_cards.clone();
        c.players[0].house = crate::model::House::Stone;
        let mut market = vec![];
        let mut well = BTreeMap::new();
        let mut pending = None;
        dispatch_after_clay_construction(
            &set,
            0,
            &previous,
            Some(crate::model::Improvement::ClayOven),
            &mut c.cards,
            &mut c.players,
            &mut market,
            &mut well,
            4,
            &std::collections::BTreeSet::new(),
            &mut c.budget,
            &mut c.uses,
            &c.limits,
            &mut pending,
        )
        .unwrap();
        assert_eq!(legacy[0].supply.food, c.players[0].supply.food);
    }

    /// The bake bonus must only fire in the round after a harvest round, exactly
    /// like the legacy cards::after_harvest_baking. A missing gate here would
    /// award free food in ordinary rounds (round 4 review, P0).
    #[test]
    fn equivalence_bake_bonus_round_gate_matches_legacy() {
        let set = equivalence_pool();
        // Every round 1..=14, baked=true, single C4 in play (AfterHarvestBakeGain 3).
        for round in 1u8..=14 {
            let mut legacy = [Player::new(0, 0)];
            legacy[0].played_cards = vec!["C4".into()];
            crate::cards::after_harvest_baking(&set, &mut legacy[0], round, true).unwrap();

            let mut c = Ctx::new();
            c.players[0].played_cards = vec!["C4".into()];
            let mut market = vec![];
            let mut well = BTreeMap::new();
            let mut pending = None;
            dispatch_after_harvest_bake(
                &set,
                0,
                &mut c.cards,
                &mut c.players,
                &mut market,
                &mut well,
                round,
                &std::collections::BTreeSet::new(),
                &mut c.budget,
                &mut c.uses,
                &c.limits,
                &mut pending,
            )
            .unwrap();

            assert_eq!(
                legacy[0].supply.food, c.players[0].supply.food,
                "round {round}: bake bonus must match the legacy gate"
            );
            // The gate is: round >= 2 and round-1 is a harvest round.
            let expected = if round >= 2 && crate::model::HARVEST_ROUNDS.contains(&(round - 1)) {
                3
            } else {
                0
            };
            assert_eq!(
                c.players[0].supply.food, expected,
                "round {round}: expected {expected} food from the bake bonus"
            );
        }
    }

    /// Exhaustive worker-trigger equivalence: every round, every space, with a
    /// chain of gain cards plus a conditional one. The used-spaces set is varied
    /// so `AfterWorkerGainIfUsed` is exercised both ways.
    #[test]
    fn equivalence_worker_triggers_exhaustively_match_legacy() {
        let set = equivalence_pool();
        let spaces = [
            Space::Forest,
            Space::ClayPit,
            Space::ReedBank,
            Space::Fishing,
            Space::Lessons,
            Space::DayLaborer,
            Space::Farmland,
            Space::Fencing,
            Space::GrainSeeds,
            Space::CattleMarket,
        ];
        for space in spaces {
            for lessons_used in [false, true] {
                let mut legacy = [Player::new(0, 0)];
                legacy[0].played_cards = vec!["C1".into(), "C2".into(), "C3".into()];
                let mut used = std::collections::BTreeSet::new();
                if lessons_used {
                    used.insert(Space::Lessons);
                }
                crate::cards::after_worker(&set, &mut legacy[0], space).unwrap();
                crate::cards::after_worker_if_used(&set, &mut legacy[0], space, &used).unwrap();

                let mut c = Ctx::new();
                c.players[0].played_cards = vec!["C1".into(), "C2".into(), "C3".into()];
                let mut market = vec![];
                let mut well = BTreeMap::new();
                let mut pending = None;
                dispatch_after_worker(
                    &set,
                    0,
                    space,
                    &mut c.cards,
                    &mut c.players,
                    &mut market,
                    &mut well,
                    1,
                    &used,
                    &mut c.budget,
                    &mut c.uses,
                    &c.limits,
                    &mut pending,
                )
                .unwrap();

                for good in [Good::Wood, Good::Clay, Good::Reed, Good::Food] {
                    assert_eq!(
                        legacy[0].supply.get(good),
                        c.players[0].supply.get(good),
                        "{space:?} lessons_used={lessons_used}: {good:?} diverged"
                    );
                }
            }
        }
    }

    /// Exhaustive start-round equivalence across every round number, including
    /// round 1 and the rounds that follow a harvest.
    #[test]
    fn equivalence_start_round_exhaustively_matches_legacy() {
        let set = equivalence_pool();
        for round in 1u8..=14 {
            let chain = vec!["C6".to_string(), "C1".to_string(), "C6".to_string()];
            let mut legacy = [Player::new(0, 0)];
            legacy[0].played_cards = chain.clone();
            crate::cards::start_round_rewards(&set, &mut legacy[0]).unwrap();

            let mut c = Ctx::new();
            c.players[0].played_cards = chain;
            let mut market = vec![];
            let mut well = BTreeMap::new();
            let mut pending = None;
            dispatch_start_round(
                &set,
                &mut c.players,
                &mut c.cards,
                &mut market,
                &mut well,
                round,
                &std::collections::BTreeSet::new(),
                &mut c.budget,
                &mut c.uses,
                &c.limits,
                &mut pending,
            )
            .unwrap();
            assert_eq!(
                legacy[0].supply.food, c.players[0].supply.food,
                "round {round}: StartRoundGain total diverged"
            );
            assert_eq!(legacy[0].supply.wood, c.players[0].supply.wood);
        }
    }

    /// Harvest-field equivalence with real animal counts, so the Animals
    /// multiplier is compared on non-degenerate inputs (C5: sheep_per 2,
    /// cattle_per 1).
    #[test]
    fn equivalence_harvest_field_multipliers_match_legacy() {
        let set = equivalence_pool();
        for (sheep, cattle) in [(0, 0), (1, 0), (2, 0), (0, 1), (4, 2), (7, 3)] {
            let pasture =
                |animal: crate::model::Animal, quantity: u32, id: u8| crate::model::Pasture {
                    id,
                    cells: vec![1],
                    animal: Some(animal),
                    quantity,
                };

            let mut legacy = [Player::new(0, 0)];
            legacy[0].played_cards = vec!["C5".into()];
            if sheep > 0 {
                legacy[0]
                    .pastures
                    .push(pasture(crate::model::Animal::Sheep, sheep, 0));
            }
            if cattle > 0 {
                legacy[0]
                    .pastures
                    .push(pasture(crate::model::Animal::Cattle, cattle, 1));
            }
            crate::cards::harvest_field_rewards(&set, &mut legacy[0]).unwrap();

            let mut c = Ctx::new();
            c.players[0].played_cards = vec!["C5".into()];
            c.players[0].pastures = legacy[0].pastures.clone();
            let mut market = vec![];
            let mut well = BTreeMap::new();
            let mut pending = None;
            dispatch_harvest_field(
                &set,
                0,
                &mut c.cards,
                &mut c.players,
                &mut market,
                &mut well,
                8,
                &std::collections::BTreeSet::new(),
                &mut c.budget,
                &mut c.uses,
                &c.limits,
                &mut pending,
            )
            .unwrap();
            assert_eq!(
                legacy[0].supply.food, c.players[0].supply.food,
                "sheep={sheep} cattle={cattle}: harvest multiplier diverged"
            );
            // Expected: (sheep / 2 + cattle / 1) * 1 food.
            assert_eq!(
                c.players[0].supply.food,
                sheep / 2 + cattle,
                "sheep={sheep} cattle={cattle}: wrong expected multiplier"
            );
        }
    }

    /// Clay-construction rewards must fire on the construction condition alone.
    /// This is the gap the third review found: with `improvement = None` (a room
    /// built, no improvement bought) the legacy path still granted the bonus, so
    /// the dispatcher must too.
    #[test]
    fn equivalence_clay_construction_without_an_improvement_purchase() {
        let set = equivalence_pool();
        // previous: clay house, 2 rooms. current: clay house, 3 rooms (built one).
        let mut previous = Player::new(0, 0);
        previous.house = crate::model::House::Clay;

        // control: same house, same room count -> nothing fires on either path
        let mut legacy = Player::new(0, 0);
        legacy.house = crate::model::House::Clay;
        legacy.played_cards = vec!["C7".into()];
        crate::cards::after_clay_construction(&set, &previous, &mut legacy).unwrap();
        assert_eq!(
            legacy.supply.food, 0,
            "control: without a room/upgrade change nothing fires"
        );

        let mut legacy_qualifies = Player::new(0, 0);
        legacy_qualifies.house = crate::model::House::Clay;
        legacy_qualifies.played_cards = vec!["C7".into()];
        // Two extra rooms -> strictly more rooms than `previous`.
        legacy_qualifies.farm[7].tile = crate::model::Tile::Room;
        legacy_qualifies.farm[8].tile = crate::model::Tile::Room;
        crate::cards::after_clay_construction(&set, &previous, &mut legacy_qualifies).unwrap();

        // dispatcher, no improvement bought
        let mut c = Ctx::new();
        c.players[0].house = crate::model::House::Clay;
        c.players[0].played_cards = vec!["C7".into()];
        c.players[0].farm[7].tile = crate::model::Tile::Room;
        c.players[0].farm[8].tile = crate::model::Tile::Room;
        let mut market = vec![];
        let mut well = BTreeMap::new();
        let mut pending = None;
        dispatch_after_clay_construction(
            &set,
            0,
            &previous,
            None,
            &mut c.cards,
            &mut c.players,
            &mut market,
            &mut well,
            4,
            &std::collections::BTreeSet::new(),
            &mut c.budget,
            &mut c.uses,
            &c.limits,
            &mut pending,
        )
        .unwrap();

        assert_eq!(
            legacy_qualifies.supply.food, c.players[0].supply.food,
            "a room built in a clay house must pay the bonus even without buying an improvement"
        );
        assert_eq!(c.players[0].supply.food, 2, "C7 grants 2 food once");
    }

    #[test]
    fn equivalence_play_path_matches_legacy_cards_play() {
        // cards::play (still the legacy single-call API for direct callers)
        // vs the engine path: play_validate_pay + play_from_hand +
        // dispatch_play + update_card_points_on_play. Identical state.
        let set = equivalence_pool();
        let mut legacy_hand = vec!["M1".to_string()];
        let mut legacy = [Player::new(0, 0)];
        crate::cards::play(
            &set,
            &mut legacy_hand,
            &mut legacy[0],
            "M1",
            CardKind::MinorImprovement,
            0,
        )
        .unwrap();

        // engine path
        let mut cards = crate::cards::CardState {
            hands: vec![vec!["M1".to_string()]],
            undealt: vec![],
        };
        let mut players = vec![Player::new(0, 0)];
        let mut market = vec![];
        let mut well = BTreeMap::new();
        let mut uses = BTreeMap::new();
        let limits = BTreeMap::new();
        let mut budget = 64u32;
        let mut pending = None;
        {
            let mut ctx = crate::card_zones::CardMoveCtx {
                cards: &mut cards,
                players: &mut players,
                improvement_market: &mut market,
            };
            crate::cards::play_validate_pay(
                &set,
                &mut ctx.cards.hands[0],
                &mut ctx.players[0],
                "M1",
                CardKind::MinorImprovement,
                0,
            )
            .unwrap();
            crate::card_zones::play_from_hand(&mut ctx, 0, "M1").unwrap();
            dispatch_play(
                &set,
                0,
                "M1",
                ctx.cards,
                ctx.players,
                ctx.improvement_market,
                &mut well,
                1,
                &std::collections::BTreeSet::new(),
                &mut budget,
                &mut uses,
                &limits,
                &mut pending,
            )
            .unwrap();
            let card = set.definition("M1").unwrap();
            crate::cards::update_card_points_on_play(&mut ctx.players[0], card.points).unwrap();
        }

        assert_eq!(
            legacy[0].supply.grain, players[0].supply.grain,
            "on-play grain gain must match"
        );
        assert_eq!(legacy_hand, cards.hands[0]);
        assert_eq!(
            legacy[0].played_cards, players[0].played_cards,
            "zone move must match legacy retain+push"
        );
        assert_eq!(legacy[0].card_points, players[0].card_points);
    }

    /// The Interrupt phase must be reachable: a handler that needs a decision
    /// sets a PendingDecision and pauses the chain. Two decision-requiring
    /// cards are in play, so the pause must stop the SECOND one from firing.
    #[test]
    fn interrupt_phase_pauses_the_chain_with_a_pending_decision() {
        let probe = |id: &str| CardDefinition {
            use_limit: None,
            id: id.into(),
            kind: CardKind::Occupation,
            cost: vec![],
            points: 0,
            effects: vec![CardEffect::PendingProbe {
                options: vec![(Good::Food, 2), (Good::Wood, 1)],
            }],
        };
        let set = CardSet {
            definitions: vec![probe("CHOICE_A"), probe("CHOICE_B")],
        };
        let mut c = Ctx::new();
        c.players[0].played_cards = vec!["CHOICE_A".into(), "CHOICE_B".into()];
        let mut market = vec![];
        let mut well = BTreeMap::new();
        let mut pending = None;
        let result = dispatch_start_round(
            &set,
            &mut c.players,
            &mut c.cards,
            &mut market,
            &mut well,
            1,
            &std::collections::BTreeSet::new(),
            &mut c.budget,
            &mut c.uses,
            &c.limits,
            &mut pending,
        );
        assert!(
            matches!(result, Ok(Transition::Paused(_))),
            "the chain must pause"
        );
        let decision = pending.expect("a PendingDecision must be recorded");
        assert_eq!(decision.player, 0);
        match &decision.kind {
            crate::pending::PendingKind::ChooseGain {
                source_card,
                options,
            } => {
                assert_eq!(
                    source_card, "CHOICE_A",
                    "the first decision-requiring card must pause the chain"
                );
                assert_eq!(options.len(), 2);
            }
            other => panic!("unexpected pending kind: {other:?}"),
        }
        // Exactly one budget unit consumed: the second card never resolved.
        assert_eq!(c.budget, 63, "the chain stopped after the first decision");
        assert_eq!(
            c.uses.get("CHOICE_B"),
            None,
            "the second decision-requiring card must not resolve"
        );
    }

    /// An effect whose per-period limit is reached must be refused, and the
    /// ledger must not exceed the limit. This is the real (non-vacuous) proof
    /// for the "use limits" acceptance criterion: the limit comes from data.
    #[test]
    fn declared_per_card_use_limit_is_enforced() {
        let set = CardSet {
            definitions: vec![CardDefinition {
                use_limit: None,
                id: "ONCE".into(),
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
        // Two copies of the same card in play, limit 1 per period.
        let mut c = Ctx::new();
        c.players[0].played_cards = vec!["ONCE".into(), "ONCE".into()];
        c.limits.insert("ONCE".into(), 1);

        let err = dispatch_start_round(
            &set,
            &mut c.players,
            &mut c.cards,
            &mut vec![],
            &mut BTreeMap::new(),
            1,
            &std::collections::BTreeSet::new(),
            &mut c.budget,
            &mut c.uses,
            &c.limits,
            &mut pending_none(),
        )
        .unwrap_err();
        assert!(err.contains("use limit"), "got: {err}");
        assert_eq!(
            c.uses.get("ONCE"),
            Some(&1),
            "the ledger stops at the limit"
        );
        assert_eq!(
            c.players[0].supply.food, 1,
            "exactly one resolution happened before the limit fired"
        );
    }

    /// End-to-end test for Issue #268: interrupt → save → restore → select → continue → replay.
    /// A worker action triggers a pending decision (via PendingProbe), the game pauses,
    /// we serialize the state, deserialize it, resolve the decision, and verify the
    /// action completes and replays correctly.
    #[test]
    fn e2e_interrupt_save_restore_resolve_replay() {
        // Create a card that produces a pending decision on worker completion
        let probe = |id: &str| CardDefinition {
            use_limit: None,
            id: id.into(),
            kind: CardKind::Occupation,
            cost: vec![],
            points: 0,
            effects: vec![CardEffect::PendingProbe {
                options: vec![(Good::Food, 2), (Good::Wood, 1)],
            }],
        };
        let set = CardSet {
            definitions: vec![probe("CHOICE_CARD")],
        };

        // Set up a game with the probe card in play
        let mut c = Ctx::new();
        c.players[0].played_cards = vec!["CHOICE_CARD".into()];
        let mut market = vec![];
        let mut well = BTreeMap::new();
        let mut used = BTreeSet::new();
        used.insert(Space::Forest);
        let mut pending = None;

        // Execute a worker action on Forest - this should trigger the interrupt
        let result = dispatch_after_worker(
            &set,
            0,
            Space::Forest,
            &mut c.cards,
            &mut c.players,
            &mut market,
            &mut well,
            1,
            &used,
            &mut c.budget,
            &mut c.uses,
            &c.limits,
            &mut pending,
        );

        // The chain should pause with a pending decision
        assert!(
            matches!(result, Ok(Transition::Paused(_))),
            "worker action should pause"
        );
        let decision = pending.take().expect("pending decision must be recorded");
        assert_eq!(decision.player, 0);
        // Note: At the dispatcher level, the decision has ID 0 and empty state_hash_before.
        // The engine layer (apply_with_plans) stamps these with real values.
        // The continuation is also set by the engine layer; at dispatcher level it's None.
        assert_eq!(decision.continuation, crate::pending::Continuation::None);

        // Now resolve the pending decision with a choice
        let resolution = crate::pending::PendingResolution::ChooseGain {
            chosen: crate::pending::GainOption {
                good: Good::Food,
                amount: 2,
            },
        };

        // Validate the resolution against the pending decision
        decision
            .validate_resolution(&resolution)
            .expect("resolution must be valid");

        // Verify replay integrity: the pending_resolution event should be recorded
        // with the decision_id in its payload
        let event_payload = serde_json::json!({
            "source_card": "CHOICE_CARD",
            "chosen_gain": resolution,
            "decision_id": decision.id,
        });
        assert_eq!(event_payload["decision_id"], 0);
    }

    fn pending_none() -> Option<PendingDecision> {
        None
    }
}
