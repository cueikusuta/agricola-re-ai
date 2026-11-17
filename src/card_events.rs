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
use crate::pending::{ChainPosition, PendingDecision, PendingKind, Transition};
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
        CardEffect::OnPlayGainPerImprovementPlayed { .. } => TriggerPhase::Before,
        // Passive effects are continuous — model as Before (no choices)
        CardEffect::Passive { .. } => TriggerPhase::Before,
        // Worker completion triggers — After phase
        CardEffect::AfterWorkerGain { .. } => TriggerPhase::After,
        CardEffect::AfterWorkerGainIfUsed { .. } => TriggerPhase::After,
        // Round start — After phase (happens after round begins)
        CardEffect::StartRoundGain { .. } => TriggerPhase::After,
        CardEffect::StartRoundGainIfFoodAtLeastRound { .. } => TriggerPhase::After,
        CardEffect::StartRoundGainIfNoMinorPlayed { .. } => TriggerPhase::After,
        // Fired by dispatch_play_after through the standard After-phase scan
        // (P1-1 unified play order: AfterPlay resolves as a normal effect).
        CardEffect::AfterOccupationGainPerFoodCost { .. } => TriggerPhase::After,
        // Clay construction purchase — After phase
        CardEffect::AfterClayConstructionGain { .. } => TriggerPhase::After,
        // Harvest/baking — After phase
        CardEffect::AfterHarvestBakeGain { .. } => TriggerPhase::After,
        CardEffect::HarvestFieldGain { .. } => TriggerPhase::After,
        CardEffect::HarvestFieldGainIfNoMinorPlayed { .. } => TriggerPhase::After,
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
    /// The player taking the action (actor). For worker placement, this is
    /// the player who placed the worker. For round start, it's the start player.
    pub actor: usize,
    /// The player who owns the card being resolved (card owner).
    /// In the standard flow, this equals `player` parameter of resolve_phase.
    pub card_owner: usize,
    /// The player(s) who benefit from the effect. For most effects, this is
    /// the card owner, but some effects target other players (e.g., "all opponents").
    /// Handlers should use this to determine the actual beneficiary.
    pub beneficiary: usize,
    /// Food paid as the current card play's cost (printed cost + occupation fee).
    /// Used by C116's AfterOccupationGainPerFoodCost handler during the
    /// AfterPlay phase of an occupation play (P1-1 unified play order).
    pub play_food_paid: u32,
}

impl<'a> DispatchContext<'a> {
    /// Default play_food_paid for dispatchers that are not a card play
    /// (after-worker, start-round, harvest, ...). The C116 AfterPlay handler
    /// only fires during a play's After phase, so a zero there keeps other
    /// sites inert.
    pub fn zero_play_food_paid() -> u32 {
        0
    }
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
///
/// Issue #270: Two-pass execution with condition evaluation first.
/// 1. First pass: collect all triggerable effects (condition holds, phase matches).
/// 2. If >=2 triggerable effects in After or Interrupt phase, pause for player to
///    choose order (2016 Lookout p.8) via PendingDecision with ChooseTriggerOrder.
/// 3. Otherwise, second pass: execute the (0 or 1) triggerable effects in play order.
///
/// The `allow_choose_trigger_order_pause` parameter controls whether a ChooseTriggerOrder
/// pause is permitted. This is only enabled for post-work chain dispatchers
/// (dispatch_after_worker, dispatch_after_clay_construction, dispatch_after_harvest_bake)
/// where the engine's resume_post_work continuation contract applies. Other dispatchers
/// (dispatch_play, dispatch_start_round, dispatch_harvest_field) pass `false` because
/// they lack a post-work chain to resume.
pub fn resolve_phase(
    ctx: &mut DispatchContext<'_>,
    player: usize,
    phase: TriggerPhase,
    trigger_data: &PhaseTriggerData,
    all_players: &[Player],
    allow_choose_trigger_order_pause: bool,
) -> Result<Transition, String> {
    let played_cards = ctx.player.played_cards.clone();
    let pause = resolve_phase_from(
        ctx,
        player,
        phase,
        trigger_data,
        &played_cards,
        None,
        all_players,
        allow_choose_trigger_order_pause,
    )?;
    Ok(pause)
}

/// Scan a phase's effects with two-pass execution (Issue #270).
/// First pass: collect triggerable effects (condition holds, use limit not exceeded).
/// Second pass: execute them, or pause for ChooseTriggerOrder if >=2 in After/Interrupt.
#[allow(clippy::too_many_arguments)]
fn resolve_phase_scan(
    ctx: &mut DispatchContext<'_>,
    player: usize,
    phase: TriggerPhase,
    trigger_data: &PhaseTriggerData,
    played_cards: &[String],
    after: Option<ChainPosition>,
    all_players: &[Player],
    allow_choose_trigger_order_pause: bool,
) -> Result<Transition, String> {
    // First pass: collect all triggerable effects (condition holds, phase matches).
    #[derive(Clone, Debug)]
    #[allow(dead_code)]
    struct TriggerableEffectInternal {
        card_id: String,
        card_index: usize,
        effect_index: usize,
        description: String,
    }

    let mut triggerable: Vec<TriggerableEffectInternal> = Vec::new();
    // Track how many effects per card_id we've collected in this pass
    let mut collected_per_card: std::collections::BTreeMap<String, u32> =
        std::collections::BTreeMap::new();

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

            // Evaluate the trigger condition FIRST (EventTrigger integration / Issue #270).
            // Only consume budget and card_use if the condition holds.
            if !effect_condition_holds(
                effect,
                trigger_data,
                ctx.player,
                ctx.actor,
                ctx.used_spaces,
                ctx.round,
            ) {
                effect_index += 1;
                continue; // Condition not met: skip this effect entirely, no budget/use consumed
            }

            // Check per-card use limit - if already at limit, effect is not triggerable
            let limit = ctx
                .card_use_limits
                .get(card_id.as_str())
                .copied()
                .unwrap_or(u32::MAX);
            let current_use = ctx.card_uses.get(card_id.as_str()).copied().unwrap_or(0);
            let collected = collected_per_card.get(card_id).copied().unwrap_or(0);
            if current_use + collected >= limit {
                effect_index += 1;
                continue; // Use limit reached (including effects collected in this pass): skip this effect entirely
            }

            // Build a description for the triggerable effect
            let description = format!("{:?}", effect);
            triggerable.push(TriggerableEffectInternal {
                card_id: card_id.clone(),
                card_index,
                effect_index,
                description,
            });
            // Increment collected count for this card_id
            *collected_per_card.entry(card_id.clone()).or_insert(0) += 1;

            effect_index += 1;
        }
        card_index += 1;
    }

    // If >=2 triggerable effects gather in the After phase, the active player
    // chooses the resolution order (2016 Lookout p.8: "when multiple card
    // effects trigger simultaneously for the active player, the active player
    // chooses the order of resolution"). This pause is restricted to the After
    // phase AND to callers inside the post-work chain
    // (`allow_choose_trigger_order_pause`), where the engine's resume contract
    // (Issue #268 PostWork continuation) can carry it. The Interrupt phase is
    // excluded deliberately: its only effect kind that wants a decision
    // (test-only PendingProbe) already pauses the chain through the classic
    // single-effect Interrupt pause, so choosing a "first" probe and then
    // pausing on it again would deadlock the resolution path.
    if triggerable.len() >= 2 && phase == TriggerPhase::After && allow_choose_trigger_order_pause {
        // Convert to public TriggerableEffect for context serialization
        let triggerable_public: Vec<TriggerableEffect> = triggerable
            .iter()
            .map(|t| TriggerableEffect {
                card_id: t.card_id.clone(),
                effect_index: t.effect_index,
                description: t.description.clone(),
            })
            .collect();

        // Build the triggers list for PendingKind (card_id, effect_index, description)
        let triggers: Vec<(String, usize, String)> = triggerable
            .iter()
            .map(|t| (t.card_id.clone(), t.effect_index, t.description.clone()))
            .collect();

        // Resume position: strictly after the LAST triggerable effect of this
        // phase. The resolution executes every triggerable effect (in the
        // player-chosen order), so the resumed scan must skip them all.
        let last = triggerable.last().expect("len >= 2 checked above");

        // Create a PendingDecision for ChooseTriggerOrder.
        // The engine's apply() stamps decision_id / state_hash_before and
        // builds the PostWork continuation from pause_step. This context
        // carries everything the resolution and the resume need:
        // - triggerable_effects: what to execute in the chosen order
        // - trigger_data: the exact phase trigger context (space/bake/...)
        //   so effects evaluate identically when re-checked at resolution
        // - card_index/effect_index: the resume position for the chain scan
        // - card_owner: whose played_cards the effects belong to
        let trigger_data_json = serde_json::to_value(trigger_data).map_err(|e| e.to_string())?;
        let mut context = serde_json::Map::new();
        context.insert(
            "triggerable_effects".into(),
            serde_json::to_value(&triggerable_public).map_err(|e| e.to_string())?,
        );
        context.insert("trigger_data".into(), trigger_data_json);
        context.insert("phase".into(), serde_json::json!(format!("{:?}", phase)));
        context.insert("card_owner".into(), serde_json::json!(player));
        context.insert("card_index".into(), serde_json::json!(last.card_index));
        context.insert("effect_index".into(), serde_json::json!(last.effect_index));

        let decision = PendingDecision::new(
            0, // Will be stamped by engine
            player,
            PendingKind::ChooseTriggerOrder { phase, triggers },
            String::new(), // Will be stamped by engine
            serde_json::Value::Object(context),
        );
        return Ok(Transition::Paused(Box::new(decision)));
    }

    // >=2 simultaneous triggers reached here only when the pause is not
    // permitted (non-post-work dispatcher, Interrupt phase, or resume): fall
    // back to play order so legacy behavior parity is preserved.
    if triggerable.len() >= 2 {
        // Fallback: auto-resolve with play order (for dispatchers without post-work chain)
        let chosen_order: Vec<usize> = (0..triggerable.len()).collect();

        // Serialize the triggerable list once for the chosen-order executor.
        let triggerable_public: Vec<TriggerableEffect> = triggerable
            .iter()
            .map(|t| TriggerableEffect {
                card_id: t.card_id.clone(),
                effect_index: t.effect_index,
                description: t.description.clone(),
            })
            .collect();

        // Build trigger data for execution
        let trigger_data_for_execute = PhaseTriggerData {
            space: trigger_data.space,
            improvement: trigger_data.improvement,
            baked: trigger_data.baked,
            previous_player: trigger_data.previous_player.clone(),
            used_spaces: trigger_data.used_spaces.clone(),
        };

        // Collect all players for snapshot
        let all_players = all_players.to_vec();

        // Build dispatch context for the card owner
        let mut exec_ctx = DispatchContext {
            card_set: ctx.card_set,
            player: ctx.player,
            cards: ctx.cards,
            improvement_market: ctx.improvement_market,
            delayed_well: ctx.delayed_well,
            round: ctx.round,
            used_spaces: ctx.used_spaces,
            event_budget: ctx.event_budget,
            card_uses: ctx.card_uses,
            card_use_limits: ctx.card_use_limits,
            pending: ctx.pending,
            actor: ctx.actor,
            card_owner: player,
            beneficiary: player,
            play_food_paid: ctx.play_food_paid,
        };

        // Execute in chosen order (play order)
        execute_chosen_triggers(
            &mut exec_ctx,
            player,
            phase,
            &trigger_data_for_execute,
            &all_players,
            &chosen_order,
            &triggerable_public,
        )?;

        // Continue to second pass for any remaining effects (should be none since we executed all)
        // But we need to skip the second pass for the effects we already executed.
        // For simplicity, we return here since execute_chosen_triggers handles all triggerable effects.
        return Ok(Transition::Completed);
    }

    // Second pass: execute the (0 or 1) triggerable effects in play order.
    let mut card_index: usize = 0;
    for card_id in played_cards {
        let Some(card) = ctx.card_set.definition(card_id) else {
            card_index += 1;
            continue;
        };

        // Set card_owner in context for this card
        ctx.card_owner = player;

        let mut effect_index: usize = 0;
        for effect in card.effects.iter() {
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

            // Re-check condition (should still hold, but verify)
            if !effect_condition_holds(
                effect,
                trigger_data,
                ctx.player,
                ctx.actor,
                ctx.used_spaces,
                ctx.round,
            ) {
                effect_index += 1;
                continue;
            }

            // Consume event budget
            consume_budget(ctx.event_budget)?;

            // Per-card use limit for the current period.
            let limit = ctx
                .card_use_limits
                .get(card_id.as_str())
                .copied()
                .unwrap_or(u32::MAX);
            check_card_use(ctx.card_uses, card_id, limit)?;

            // For effects that target other players (beneficiary != card_owner),
            // set the beneficiary in context. Default is card_owner.
            ctx.beneficiary = player;
            // Read before the match: the handler arms re-borrow ctx mutably, so a
            // copy of the round number is needed for round-conditioned effects.
            let round_now = ctx.round;

            // Dispatch to handler
            let result = match effect {
                CardEffect::AfterOccupationGainPerFoodCost { good, amount } => {
                    handle_after_occupation_gain_per_food_cost(ctx, good, *amount)?
                }
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
                CardEffect::OnPlayGainPerImprovementPlayed { good, amount } => {
                    handle_on_play_gain_per_improvement_played(ctx, player, good, *amount)?
                }
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
                CardEffect::StartRoundGainIfFoodAtLeastRound { good, amount } => {
                    handle_start_round_gain_if_food_at_least_round(
                        ctx, player, good, *amount, round_now,
                    )?
                }
                CardEffect::StartRoundGainIfNoMinorPlayed { good, amount } => {
                    handle_gain_if_no_minor_played(ctx, player, good, *amount)?
                }
                CardEffect::HarvestFieldGainIfNoMinorPlayed { good, amount } => {
                    handle_gain_if_no_minor_played(ctx, player, good, *amount)?
                }
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
#[allow(clippy::too_many_arguments)]
pub fn resolve_phase_from(
    ctx: &mut DispatchContext<'_>,
    player: usize,
    phase: TriggerPhase,
    trigger_data: &PhaseTriggerData,
    played_cards: &[String],
    after: Option<ChainPosition>,
    all_players: &[Player],
    allow_choose_trigger_order_pause: bool,
) -> Result<Transition, String> {
    resolve_phase_scan(
        ctx,
        player,
        phase,
        trigger_data,
        played_cards,
        after,
        all_players,
        allow_choose_trigger_order_pause,
    )
}

/// Execute all effects of a given phase for ONLY the specified card (newly played).
/// This is used by dispatch_play to avoid re-firing already-played cards' effects.
fn resolve_phase_for_card(
    ctx: &mut DispatchContext<'_>,
    player: usize,
    phase: TriggerPhase,
    trigger_data: &PhaseTriggerData,
    card_id: &str,
    all_players: &[Player],
) -> Result<Transition, String> {
    let Some(_card) = ctx.card_set.definition(card_id) else {
        return Ok(Transition::Completed); // Card not found, nothing to do
    };

    // Single-card scan: the position only tracks the effect index within
    // THIS card, and the pause reports it via `paused_effect` so the caller
    // can build an accurate resume position (card_index comes from context).
    let paused_effect: Option<usize> = None;
    let card_id_owned = card_id.to_string();
    let played_cards = std::slice::from_ref(&card_id_owned);
    let result = resolve_phase_scan(
        ctx,
        player,
        phase,
        trigger_data,
        played_cards,
        None,
        all_players,
        false, // resolve_phase_for_card is for card play, not post-work chain
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
#[derive(Debug, Clone, Serialize, Deserialize)]
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

/// Check if an effect's trigger condition holds, without consuming budget or card uses.
/// This is the "condition evaluation first" pass (EventTrigger integration / Issue #270).
/// Only effects whose condition holds proceed to budget/use consumption and execution.
fn effect_condition_holds(
    effect: &CardEffect,
    trigger: &PhaseTriggerData,
    player: &Player,
    _actor: usize,
    used_spaces: &std::collections::BTreeSet<Space>,
    round: u8,
) -> bool {
    match effect {
        CardEffect::AfterWorkerGain { space, .. } => trigger.space == Some(*space),
        CardEffect::AfterWorkerGainIfUsed {
            space,
            required_space,
            ..
        } => trigger.space == Some(*space) && used_spaces.contains(required_space),
        CardEffect::StartRoundGain { house, rooms, .. } => {
            let house_ok = house.is_none_or(|h| player.house == h);
            let rooms_ok = rooms.is_none_or(|r| player.rooms() == usize::from(r));
            house_ok && rooms_ok
        }
        CardEffect::StartRoundGainIfFoodAtLeastRound { .. } => {
            // C111 小動物飼育員: the threshold is the current round number
            // (e.g. 8+ food in round 8); the handler grants the gain.
            player.supply.food >= u32::from(round)
        }
        // E111 世捨て人: the no-minor-improvements condition needs card
        // definitions, so the scan reports it as triggerable and the
        // handler performs the check.
        CardEffect::StartRoundGainIfNoMinorPlayed { .. } => true,
        CardEffect::HarvestFieldGainIfNoMinorPlayed { .. } => true,
        // C116 調度品職人: fires during a card play's AfterPlay phase when the
        // play paid food as its occupation cost (P1-1 unified play order).
        // The handler re-checks play_food_paid > 0 against the context, so the
        // scan reports it as triggerable whenever the phase is a card play.
        CardEffect::AfterOccupationGainPerFoodCost { .. } => true,
        CardEffect::AfterClayConstructionGain { .. } => {
            let Some(prev) = &trigger.previous_player else {
                return false;
            };
            (prev.house == House::Clay && player.rooms() > prev.rooms())
                || (prev.house == House::Clay && player.house == House::Stone)
        }
        CardEffect::AfterHarvestBakeGain { .. } => trigger.baked,
        CardEffect::HarvestFieldGain { .. } => true,
        CardEffect::OnPlayGain { .. } => true,
        CardEffect::OnPlayGainPerStored { .. } => true,
        CardEffect::OnPlayGainPerImprovementPlayed { .. } => true,
        CardEffect::OnPlayGainIfHouse { house, rooms, .. } => {
            player.house == *house && player.rooms() == usize::from(*rooms)
        }
        CardEffect::Passive { .. } => true,
        CardEffect::Prerequisite { .. } => true,
        CardEffect::EndGameBonus { .. } => true,
        #[cfg(test)]
        CardEffect::PendingProbe { .. } => true,
    }
}

/// A triggerable effect that can be serialized for pending decision context.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TriggerableEffect {
    pub card_id: String,
    pub effect_index: usize,
    pub description: String,
}

/// Execute a specific list of effects in the given order.
/// Used when the active player has chosen a trigger resolution order via PendingDecision.
pub fn execute_chosen_triggers(
    ctx: &mut DispatchContext<'_>,
    player: usize,
    phase: TriggerPhase,
    trigger_data: &PhaseTriggerData,
    _all_players: &[Player],
    chosen_order: &[usize],
    triggerable: &[TriggerableEffect],
) -> Result<(), String> {
    // Map card_id to its definition for quick lookup
    let mut card_defs: std::collections::BTreeMap<String, &crate::cards::CardDefinition> =
        std::collections::BTreeMap::new();
    for card_id in &ctx.player.played_cards {
        if let Some(card) = ctx.card_set.definition(card_id) {
            card_defs.insert(card_id.clone(), card);
        }
    }

    for &idx in chosen_order {
        let trigger = &triggerable[idx];
        let Some(card) = card_defs.get(&trigger.card_id) else {
            continue; // Card not found, skip
        };
        if trigger.effect_index >= card.effects.len() {
            continue; // Invalid index, skip
        }

        let effect = &card.effects[trigger.effect_index];

        // Double-check phase matches
        if effect_phase(effect) != phase {
            continue;
        }

        // Double-check condition (should still hold, but verify)
        if !effect_condition_holds(
            effect,
            trigger_data,
            ctx.player,
            ctx.actor,
            ctx.used_spaces,
            ctx.round,
        ) {
            continue;
        }

        // Consume event budget
        consume_budget(ctx.event_budget)?;

        // Check card use limit
        let limit = ctx
            .card_use_limits
            .get(&trigger.card_id)
            .copied()
            .unwrap_or(u32::MAX);
        check_card_use(ctx.card_uses, &trigger.card_id, limit)?;

        ctx.card_owner = player;
        ctx.beneficiary = player;
        // Read before the match: the handler arms re-borrow ctx mutably, so a
        // copy of the round number is needed for round-conditioned effects.
        let round_now = ctx.round;

        // Dispatch to handler
        let result = match effect {
            CardEffect::AfterOccupationGainPerFoodCost { good, amount } => {
                handle_after_occupation_gain_per_food_cost(ctx, good, *amount)?
            }
            #[cfg(test)]
            CardEffect::PendingProbe { options } => {
                handle_pending_probe(ctx, player, &trigger.card_id, options.clone(), trigger_data)?
            }
            CardEffect::Prerequisite { rule } => {
                handle_prerequisite(ctx, player, &trigger.card_id, rule, trigger_data)?
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
            CardEffect::OnPlayGainPerImprovementPlayed { good, amount } => {
                handle_on_play_gain_per_improvement_played(ctx, player, good, *amount)?
            }
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
            CardEffect::StartRoundGainIfFoodAtLeastRound { good, amount } => {
                handle_start_round_gain_if_food_at_least_round(
                    ctx, player, good, *amount, round_now,
                )?
            }
            CardEffect::StartRoundGainIfNoMinorPlayed { good, amount } => {
                handle_gain_if_no_minor_played(ctx, player, good, *amount)?
            }
            CardEffect::HarvestFieldGainIfNoMinorPlayed { good, amount } => {
                handle_gain_if_no_minor_played(ctx, player, good, *amount)?
            }
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
                "card": trigger.card_id,
                "card_index": 0, // Single-card execution context
                "effect_index": trigger.effect_index,
            });
            *ctx.pending = Some(pending.clone());
            return Err("pending decision".into()); // Signal to caller to pause
        }
    }

    Ok(())
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

fn handle_on_play_gain_per_improvement_played(
    ctx: &mut DispatchContext<'_>,
    _player: usize,
    good: &Good,
    amount: u32,
) -> Result<EffectResult, String> {
    let count = crate::cards::improvement_count_in_front(ctx.card_set, ctx.player);
    let bonus = count
        .checked_mul(amount)
        .ok_or("on-play card bonus overflow")?;
    let p = &mut ctx.player;
    let next = p
        .supply
        .get(*good)
        .checked_add(bonus)
        .ok_or("resource overflow")?;
    p.supply.set(*good, next);
    Ok(EffectResult::Resolved)
}

/// C116 調度品職人 (Instrument Craftsman): each later occupation play grants
/// `amount` of `good` per food paid as that play's occupation cost (printed
/// cost + the optional occupation fee). This is the AfterPlay arm of the
/// unified play order (P1-1): it fires through the generic After-phase scan
/// via `dispatch_play_after`, so the play_food_paid context carries the cost.
/// Outside a card play (after-worker, start-round, harvest, ...) the context's
/// `play_food_paid` is 0 and the effect resolves inertly.
fn handle_after_occupation_gain_per_food_cost(
    ctx: &mut DispatchContext<'_>,
    good: &Good,
    amount: u32,
) -> Result<EffectResult, String> {
    let food_paid = ctx.play_food_paid;
    if food_paid == 0 {
        return Ok(EffectResult::Resolved);
    }
    let total = amount
        .checked_mul(food_paid)
        .ok_or("occupation-play bonus overflow")?;
    let p = &mut ctx.player;
    let next = p
        .supply
        .get(*good)
        .checked_add(total)
        .ok_or("resource overflow")?;
    p.supply.set(*good, next);
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

/// C111 小動物飼育員: at the round-start trigger, grant the gain when the
/// owner's food stock is at least the current round number (e.g. 8+ food in
/// round 8). The condition itself was already checked by
/// `effect_condition_holds`; re-checking here keeps the handler correct for
/// every call site (execute_chosen_triggers included).
fn handle_start_round_gain_if_food_at_least_round(
    ctx: &mut DispatchContext<'_>,
    _player: usize,
    good: &Good,
    amount: u32,
    round: u8,
) -> Result<EffectResult, String> {
    let p = &mut ctx.player;
    if p.supply.food >= u32::from(round) {
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
        crate::cards::HarvestFieldRule::VegetablesHarvested => {
            // Same definition as cards::vegetable_yield: the dispatcher runs
            // before crops are removed, so growing vegetable fields equal the
            // vegetables this field phase will yield (one per field).
            p.farm
                .iter()
                .filter(|cell| {
                    matches!(
                        cell.tile,
                        crate::model::Tile::Field {
                            crop: Some(Crop::Vegetable),
                            remaining: 1..,
                        }
                    )
                })
                .count() as u32
        }
        crate::cards::HarvestFieldRule::HasWheatField => u32::from(p.farm.iter().any(|cell| {
            matches!(
                cell.tile,
                crate::model::Tile::Field {
                    crop: Some(Crop::Grain),
                    ..
                }
            )
        })),
        crate::cards::HarvestFieldRule::FieldCountThresholds {
            thresholds,
            amounts,
        } => {
            let fields = p
                .farm
                .iter()
                .filter(|cell| matches!(cell.tile, crate::model::Tile::Field { .. }))
                .count() as u32;
            let mut best = 0;
            for (threshold, gained) in thresholds.iter().zip(amounts.iter()) {
                if fields >= *threshold {
                    best = *gained;
                }
            }
            best
        }
        crate::cards::HarvestFieldRule::LastGrainTaken
        | crate::cards::HarvestFieldRule::LastVegetableTaken => {
            let crop = if rule == &crate::cards::HarvestFieldRule::LastGrainTaken {
                Crop::Grain
            } else {
                Crop::Vegetable
            };
            p.farm
                .iter()
                .filter(|cell| {
                    matches!(
                        cell.tile,
                        crate::model::Tile::Field {
                            crop: Some(found),
                            remaining: 1,
                        } if found == crop
                    )
                })
                .count() as u32
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

/// E111 世捨て人: shared gain logic for its round-start and harvest-start
/// halves. The no-minor-improvements condition needs card definitions, so it
/// is checked here rather than in the generic condition scan.
fn handle_gain_if_no_minor_played(
    ctx: &mut DispatchContext<'_>,
    _player: usize,
    good: &Good,
    amount: u32,
) -> Result<EffectResult, String> {
    let played = ctx.player.played_cards.clone();
    let condition = played.iter().all(|id| {
        ctx.card_set
            .definition(id)
            .is_none_or(|card| card.kind != crate::cards::CardKind::MinorImprovement)
    });
    if condition {
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
    #[allow(unused_variables)] extra_food_paid: u32,
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
    // Clone players for all_players parameter to avoid borrow checker issues
    let all_players = players.to_vec();
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
        actor: player,
        card_owner: player,
        beneficiary: player,
        play_food_paid: 0,
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
        &all_players,
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
        &all_players,
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

    // Phase 4: After — handled by caller via dispatch_play_after to ensure
    // the card is in played_cards and ChooseTriggerOrder works correctly.

    Ok(())
}

/// Dispatch the After phase for a card play. This runs After effects
/// (including C116's AfterOccupationGainPerFoodCost) through the standard
/// resolve_phase mechanism so that ChooseTriggerOrder ordering works correctly.
/// The card must already be in played_cards (caller must have run play_from_hand).
#[allow(clippy::too_many_arguments)]
pub fn dispatch_play_after(
    card_set: &CardSet,
    player: usize,
    new_card_id: &str,
    extra_food_paid: u32,
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
    // C116's trigger amount is the food paid as this occupation's cost:
    // printed food cost + the optional occupation fee.
    let printed_food: u32 = card_set
        .definition(new_card_id)
        .map(|card| {
            card.cost
                .iter()
                .filter(|(good, _)| *good == Good::Food)
                .map(|(_, n)| *n)
                .sum()
        })
        .unwrap_or(0);
    let play_food_paid = printed_food.saturating_add(extra_food_paid);
    // Clone players for all_players parameter to avoid borrow checker issues
    let all_players = players.to_vec();
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
        actor: player,
        card_owner: player,
        beneficiary: player,
        play_food_paid,
    };

    let trigger = PhaseTriggerData {
        space: None,
        improvement: None,
        baked: false,
        previous_player: None,
        used_spaces: std::collections::BTreeSet::new(),
    };

    // Phase 4: After — the new card is now in played_cards, so After effects
    // from earlier cards (like C116) will trigger. We use allow_choose_trigger_order_pause=false
    // because dispatch_play_after is not part of a post-work chain with resume support.
    // However, if multiple After effects trigger, they still need to be ordered.
    // Since we can't pause here, we fall back to play order (per resolve_phase_scan logic).
    resolve_phase(
        &mut ctx,
        player,
        TriggerPhase::After,
        &trigger,
        &all_players,
        false, // no pause support in card play dispatcher
    )
}

/// Dispatch AfterWorker phase for a completed worker action.
/// If `allow_choose_trigger_order_pause` is true, enables ChooseTriggerOrder pause
/// for post-work chain integration (Issue #270).
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
    allow_choose_trigger_order_pause: bool,
) -> Result<Transition, String> {
    // Clone players for all_players parameter to avoid borrow checker issues
    let all_players = players.to_vec();
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
        actor: player,
        card_owner: player,
        beneficiary: player,
        play_food_paid: 0,
    };

    let trigger = PhaseTriggerData {
        space: Some(space),
        improvement: None,
        baked: false,
        previous_player: None,
        used_spaces: used_spaces.clone(),
    };

    // AfterWorkerGain and AfterWorkerGainIfUsed are After phase. A pause here
    // (ChooseTriggerOrder, Issue #270) must reach the engine so it can build
    // the PostWork continuation — propagate instead of discarding.
    if let Transition::Paused(p) = resolve_phase(
        &mut ctx,
        player,
        TriggerPhase::After,
        &trigger,
        &all_players,
        allow_choose_trigger_order_pause,
    )? {
        return Ok(Transition::Paused(p));
    }
    // Interrupts pause the chain after the passive phase has resolved
    // (2016 Lookout p.8: the active player resolves their effects in the
    // order they choose; a choice-requiring effect stops the chain).
    resolve_phase(
        &mut ctx,
        player,
        TriggerPhase::Interrupt,
        &trigger,
        &all_players,
        allow_choose_trigger_order_pause,
    )
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
    // Clone players for all_players parameter to avoid borrow checker issues
    let all_players = players.to_vec();
    // Get actor (start player) ID before the loop to avoid borrow checker issues
    let actor = all_players[0].id;
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
            actor,
            card_owner: player,
            beneficiary: player,
            play_food_paid: 0,
        };

        let trigger = PhaseTriggerData {
            space: None,
            improvement: None,
            baked: false,
            previous_player: None,
            used_spaces: used_spaces.clone(),
        };

        resolve_phase(
            &mut ctx,
            player,
            TriggerPhase::After,
            &trigger,
            &all_players,
            false, // start-round: not post-work chain
        )?;
        // Then the interrupt phase for this player. A pause here returns
        // immediately (mid-start-round); the round-start path stays typed:
        // the caller records the error rather than committing a partial
        // start_round, per Issue #268 scope (post-work chain only).
        if let Transition::Paused(p) = resolve_phase(
            &mut ctx,
            player,
            TriggerPhase::Interrupt,
            &trigger,
            &all_players,
            false, // start-round: not post-work chain
        )? {
            return Ok(Transition::Paused(p));
        }
    }
    Ok(Transition::Completed)
}

/// Dispatch AfterClayConstruction phase when a major improvement is purchased.
/// If `allow_choose_trigger_order_pause` is true, enables ChooseTriggerOrder pause
/// for post-work chain integration (Issue #270).
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
    allow_choose_trigger_order_pause: bool,
) -> Result<Transition, String> {
    // Clone players for all_players parameter to avoid borrow checker issues
    let all_players = players.to_vec();
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
        actor: player,
        card_owner: player,
        beneficiary: player,
        play_food_paid: 0,
    };

    let trigger = PhaseTriggerData {
        space: None,
        improvement,
        baked: false,
        previous_player: Some(previous.clone()),
        used_spaces: used_spaces.clone(),
    };

    // After phase first: a ChooseTriggerOrder pause (Issue #270) must reach
    // the engine — propagate instead of discarding.
    if let Transition::Paused(p) = resolve_phase(
        &mut ctx,
        player,
        TriggerPhase::After,
        &trigger,
        &all_players,
        allow_choose_trigger_order_pause,
    )? {
        return Ok(Transition::Paused(p));
    }
    // Interrupts pause the chain after the passive phase has resolved
    // (2016 Lookout p.8: the active player resolves their effects in the
    // order they choose; a choice-requiring effect stops the chain).
    if let Transition::Paused(p) = resolve_phase(
        &mut ctx,
        player,
        TriggerPhase::Interrupt,
        &trigger,
        &all_players,
        allow_choose_trigger_order_pause,
    )? {
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
    // Clone players for all_players parameter to avoid borrow checker issues
    let all_players = players.to_vec();
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
        actor: player,
        card_owner: player,
        beneficiary: player,
        play_food_paid: 0,
    };

    let trigger = PhaseTriggerData {
        space: None,
        improvement: None,
        baked: false,
        previous_player: None,
        used_spaces: used_spaces.clone(),
    };

    resolve_phase(
        &mut ctx,
        player,
        TriggerPhase::After,
        &trigger,
        &all_players,
        false, // harvest-field: not post-work chain
    )?;
    // Interrupts pause the chain after the passive phase has resolved
    // (2016 Lookout p.8: the active player resolves their effects in the
    // order they choose; a choice-requiring effect stops the chain).
    if let Transition::Paused(p) = resolve_phase(
        &mut ctx,
        player,
        TriggerPhase::Interrupt,
        &trigger,
        &all_players,
        false, // harvest-field: not post-work chain
    )? {
        return Ok(Transition::Paused(p));
    }
    Ok(Transition::Completed)
}

/// Dispatch AfterHarvestBake phase when baking occurs.
/// If `allow_choose_trigger_order_pause` is true, enables ChooseTriggerOrder pause
/// for post-work chain integration (Issue #270).
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
    allow_choose_trigger_order_pause: bool,
) -> Result<Transition, String> {
    // Same gate as the legacy cards::after_harvest_baking: a bake bonus only
    // exists in the round immediately after a harvest round. Without this the
    // dispatcher would award bake bonuses in rounds where the legacy path (and
    // every existing test) awards nothing.
    if round < 2 || !crate::model::HARVEST_ROUNDS.contains(&(round - 1)) {
        return Ok(Transition::Completed);
    }
    // Clone players for all_players parameter to avoid borrow checker issues
    let all_players = players.to_vec();
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
        actor: player,
        card_owner: player,
        beneficiary: player,
        play_food_paid: 0,
    };

    let trigger = PhaseTriggerData {
        space: None,
        improvement: None,
        baked: true,
        previous_player: None,
        used_spaces: used_spaces.clone(),
    };

    // After phase first: a ChooseTriggerOrder pause (Issue #270) must reach
    // the engine — propagate instead of discarding.
    if let Transition::Paused(p) = resolve_phase(
        &mut ctx,
        player,
        TriggerPhase::After,
        &trigger,
        &all_players,
        allow_choose_trigger_order_pause,
    )? {
        return Ok(Transition::Paused(p));
    }
    // Interrupts pause the chain after the passive phase has resolved
    // (2016 Lookout p.8: the active player resolves their effects in the
    // order they choose; a choice-requiring effect stops the chain).
    if let Transition::Paused(p) = resolve_phase(
        &mut ctx,
        player,
        TriggerPhase::Interrupt,
        &trigger,
        &all_players,
        allow_choose_trigger_order_pause,
    )? {
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
                CardEffect::OnPlayGainPerImprovementPlayed { good, amount } => {
                    let count = crate::cards::improvement_count_in_front(set, player);
                    let bonus = count
                        .checked_mul(*amount)
                        .ok_or("on-play card bonus overflow")?;
                    let next = player
                        .supply
                        .get(*good)
                        .checked_add(bonus)
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
                        crate::cards::HarvestFieldRule::VegetablesHarvested => {
                            // Same definition as cards::vegetable_yield: the
                            // dispatcher runs before crops are removed, so
                            // growing vegetable fields equal the vegetables
                            // this field phase will yield (one per field).
                            player
                                .farm
                                .iter()
                                .filter(|cell| {
                                    matches!(
                                        cell.tile,
                                        crate::model::Tile::Field {
                                            crop: Some(crate::model::Crop::Vegetable),
                                            remaining: 1..,
                                        }
                                    )
                                })
                                .count() as u32
                        }
                        crate::cards::HarvestFieldRule::HasWheatField => {
                            u32::from(player.farm.iter().any(|cell| {
                                matches!(
                                    cell.tile,
                                    crate::model::Tile::Field {
                                        crop: Some(crate::model::Crop::Grain),
                                        ..
                                    }
                                )
                            }))
                        }
                        crate::cards::HarvestFieldRule::FieldCountThresholds {
                            thresholds,
                            amounts,
                        } => {
                            let fields = player
                                .farm
                                .iter()
                                .filter(|cell| {
                                    matches!(cell.tile, crate::model::Tile::Field { .. })
                                })
                                .count() as u32;
                            let mut best = 0;
                            for (threshold, gained) in thresholds.iter().zip(amounts.iter()) {
                                if fields >= *threshold {
                                    best = *gained;
                                }
                            }
                            best
                        }
                        crate::cards::HarvestFieldRule::LastGrainTaken
                        | crate::cards::HarvestFieldRule::LastVegetableTaken => {
                            let crop = if rule == &crate::cards::HarvestFieldRule::LastGrainTaken {
                                crate::model::Crop::Grain
                            } else {
                                crate::model::Crop::Vegetable
                            };
                            player
                                .farm
                                .iter()
                                .filter(|cell| {
                                    matches!(
                                        cell.tile,
                                        crate::model::Tile::Field {
                                            crop: Some(found),
                                            remaining: 1,
                                        } if found == crop
                                    )
                                })
                                .count() as u32
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
                    crate::cards::ScoreBonusRule::CategoriesAtMaxFour => {
                        // Same rule as cards.rs: canonical category values from
                        // score.rs; count categories that reached their 4-point max.
                        let categories = &crate::score::score(player).categories;
                        [
                            "fields",
                            "pastures",
                            "grain",
                            "vegetables",
                            "sheep",
                            "boar",
                            "cattle",
                            "fenced_stables",
                        ]
                        .iter()
                        .filter(|name| categories.get(**name).copied().unwrap_or(0) >= 4)
                        .count() as i32
                    }
                    crate::cards::ScoreBonusRule::LargePastures => (player
                        .pastures
                        .iter()
                        .filter(|pasture| pasture.cells.len() >= 3)
                        .count()
                        as i32)
                        .saturating_mul(2),
                    crate::cards::ScoreBonusRule::CookingImprovements => player
                        .improvements
                        .iter()
                        .filter(|improvement| improvement.is_cooking())
                        .count()
                        as i32,
                },
                _ => 0,
            })
            .sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{
        verified_band_a_cards, verified_band_c_cards, CardDefinition, CardKind, CardRequirement,
        CardSet,
    };
    use crate::model::{Crop, Good, Improvement, Player, Space, Tile};
    use crate::pending::Transition;
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
            false, // test: not post-work chain
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
                false, // test: not post-work chain
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
            false, // test: not post-work chain
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
            false, // test: not post-work chain
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
                false, // test: not post-work chain
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
                    false, // test: not post-work chain
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
            false, // test: not post-work chain
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
                0,
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
    /// sets a PendingDecision and pauses the chain. With a single decision-requiring
    /// card in play, the chain pauses on that card's effect.
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
            definitions: vec![probe("CHOICE_A")],
        };
        let mut c = Ctx::new();
        c.players[0].played_cards = vec!["CHOICE_A".into()];
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
        // With a single PendingProbe effect, the chain pauses on that effect.
        assert!(
            matches!(result, Ok(Transition::Paused(_))),
            "the chain must pause, got: {result:?}"
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
                    "the decision-requiring card must pause the chain"
                );
                assert_eq!(options.len(), 2);
            }
            other => panic!("unexpected pending kind: {other:?}"),
        }
        // Exactly one budget unit consumed.
        assert_eq!(c.budget, 63, "the chain stopped after the decision");
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
            false, // test: not post-work chain
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

    // --- Ported tests from c0e53e4 (Issue #272 original head) ---

    /// Test that AfterWorkerGain only consumes budget/use when condition met.
    #[test]
    fn conditional_effect_not_consumed_when_condition_unmet_real_path() {
        // Card that only triggers on Fishing space
        let set = CardSet {
            definitions: vec![CardDefinition {
                use_limit: None,
                id: "FISHING_ONLY".into(),
                kind: CardKind::Occupation,
                cost: vec![],
                points: 0,
                effects: vec![CardEffect::AfterWorkerGain {
                    space: Space::Fishing,
                    good: Good::Food,
                    amount: 2,
                }],
            }],
        };

        let mut players = vec![Player::new(0, 0)];
        players[0].played_cards.push("FISHING_ONLY".into());
        let mut cards = crate::cards::CardState::default();
        let mut market = vec![];
        let mut well = BTreeMap::new();
        let mut uses = BTreeMap::new();
        let limits = BTreeMap::new();
        let mut budget = 64u32;
        let mut pending = None;

        // Activate Forest (NOT Fishing) - the Fishing-only effect should NOT fire
        // and should NOT consume budget or card use
        dispatch_after_worker(
            &set,
            0,
            Space::Forest,
            &mut cards,
            &mut players,
            &mut market,
            &mut well,
            1,
            &std::collections::BTreeSet::new(),
            &mut budget,
            &mut uses,
            &limits,
            &mut pending,
            false, // test: not post-work chain
        )
        .unwrap();

        // No food gained because condition (space == Fishing) not met
        assert_eq!(
            players[0].supply.food, 0,
            "Fishing-only effect must not fire on Forest"
        );
        // Budget must NOT be consumed when condition is unmet
        assert_eq!(
            budget, 64,
            "event budget must not be consumed when condition unmet"
        );
        // Card use must NOT be counted when condition is unmet
        assert_eq!(
            uses.get("FISHING_ONLY"),
            None,
            "card use must not be counted when condition unmet"
        );

        // Now activate Fishing - the effect SHOULD fire
        dispatch_after_worker(
            &set,
            0,
            Space::Fishing,
            &mut cards,
            &mut players,
            &mut market,
            &mut well,
            1,
            &std::collections::BTreeSet::new(),
            &mut budget,
            &mut uses,
            &limits,
            &mut pending,
            false, // test: not post-work chain
        )
        .unwrap();

        // Food gained because condition met
        assert_eq!(
            players[0].supply.food, 2,
            "Fishing-only effect must fire on Fishing"
        );
        // Budget consumed
        assert_eq!(budget, 63, "event budget consumed when condition met");
        // Card use counted
        assert_eq!(
            uses.get("FISHING_ONLY"),
            Some(&1),
            "card use counted when condition met"
        );
    }

    /// Test that AfterWorkerGainIfUsed only consumes budget/use when BOTH conditions met.
    #[test]
    fn conditional_effect_if_used_not_consumed_when_required_space_unused() {
        let set = CardSet {
            definitions: vec![CardDefinition {
                use_limit: None,
                id: "IF_USED".into(),
                kind: CardKind::Occupation,
                cost: vec![],
                points: 0,
                effects: vec![CardEffect::AfterWorkerGainIfUsed {
                    space: Space::Forest,
                    required_space: Space::Lessons,
                    good: Good::Wood,
                    amount: 3,
                }],
            }],
        };

        let mut players = vec![Player::new(0, 0)];
        players[0].played_cards.push("IF_USED".into());
        let mut cards = crate::cards::CardState::default();
        let mut market = vec![];
        let mut well = BTreeMap::new();
        let mut uses = BTreeMap::new();
        let limits = BTreeMap::new();
        let mut budget = 64u32;
        let mut pending = None;

        // Activate Forest but Lessons NOT used - effect should NOT fire
        let mut used_spaces = std::collections::BTreeSet::new();
        used_spaces.insert(Space::Forest);
        dispatch_after_worker(
            &set,
            0,
            Space::Forest,
            &mut cards,
            &mut players,
            &mut market,
            &mut well,
            1,
            &used_spaces,
            &mut budget,
            &mut uses,
            &limits,
            &mut pending,
            false, // test: not post-work chain
        )
        .unwrap();

        assert_eq!(
            players[0].supply.wood, 0,
            "IfUsed effect must not fire when required space unused"
        );
        assert_eq!(
            budget, 64,
            "budget not consumed when IfUsed condition unmet"
        );
        assert_eq!(
            uses.get("IF_USED"),
            None,
            "card use not counted when IfUsed condition unmet"
        );

        // Activate Forest WITH Lessons used - effect SHOULD fire
        used_spaces.insert(Space::Lessons);
        dispatch_after_worker(
            &set,
            0,
            Space::Forest,
            &mut cards,
            &mut players,
            &mut market,
            &mut well,
            1,
            &used_spaces,
            &mut budget,
            &mut uses,
            &limits,
            &mut pending,
            false, // test: not post-work chain
        )
        .unwrap();

        assert_eq!(
            players[0].supply.wood, 3,
            "IfUsed effect fires when required space used"
        );
        assert_eq!(budget, 63, "budget consumed when IfUsed condition met");
        assert_eq!(
            uses.get("IF_USED"),
            Some(&1),
            "card use counted when IfUsed condition met"
        );
    }

    /// Test that StartRoundGain with house/room condition only consumes budget/use when met.
    #[test]
    fn conditional_effect_start_round_not_consumed_when_house_mismatch() {
        let set = CardSet {
            definitions: vec![CardDefinition {
                use_limit: None,
                id: "STONE_HOUSE_ONLY".into(),
                kind: CardKind::Occupation,
                cost: vec![],
                points: 0,
                effects: vec![CardEffect::StartRoundGain {
                    good: Good::Food,
                    amount: 5,
                    house: Some(House::Stone),
                    rooms: None,
                }],
            }],
        };

        // Player has Wood house (not Stone)
        let mut players = vec![Player::new(0, 0)];
        players[0].played_cards.push("STONE_HOUSE_ONLY".into());
        let mut cards = crate::cards::CardState::default();
        let mut market = vec![];
        let mut well = BTreeMap::new();
        let mut uses = BTreeMap::new();
        let limits = BTreeMap::new();
        let mut budget = 64u32;
        let mut pending = None;

        dispatch_start_round(
            &set,
            &mut players,
            &mut cards,
            &mut market,
            &mut well,
            1,
            &std::collections::BTreeSet::new(),
            &mut budget,
            &mut uses,
            &limits,
            &mut pending,
        )
        .unwrap();

        assert_eq!(
            players[0].supply.food, 0,
            "Stone-house effect must not fire for Wood house"
        );
        assert_eq!(budget, 64, "budget not consumed when house condition unmet");
        assert_eq!(
            uses.get("STONE_HOUSE_ONLY"),
            None,
            "card use not counted when house condition unmet"
        );

        // Upgrade to Stone house - effect SHOULD fire
        players[0].house = House::Stone;
        dispatch_start_round(
            &set,
            &mut players,
            &mut cards,
            &mut market,
            &mut well,
            2,
            &std::collections::BTreeSet::new(),
            &mut budget,
            &mut uses,
            &limits,
            &mut pending,
        )
        .unwrap();

        assert_eq!(
            players[0].supply.food, 5,
            "Stone-house effect fires for Stone house"
        );
        assert_eq!(budget, 63, "budget consumed when house condition met");
        assert_eq!(
            uses.get("STONE_HOUSE_ONLY"),
            Some(&1),
            "card use counted when house condition met"
        );
    }

    /// Real-card end-to-end test for A078 (Canoe / カヌー) via direct CardSet.
    /// A078 is a Minor Improvement: Fishing action gives +1 food and +1 reed.
    /// This test exercises the condition-gated resolve_phase in the engine path (dispatch_after_worker),
    /// not just raw dispatchers.
    #[test]
    fn a078_canoe_e2e_via_verified_registry_pool() {
        // Create CardSet with A078 card definition directly.
        // A078: Minor Improvement with two AfterWorkerGain effects on Fishing space:
        // +1 Food and +1 Reed.
        let set = CardSet {
            definitions: vec![CardDefinition {
                use_limit: None,
                id: "A078".into(),
                kind: CardKind::MinorImprovement,
                cost: vec![],
                points: 0,
                effects: vec![
                    CardEffect::AfterWorkerGain {
                        space: Space::Fishing,
                        good: Good::Food,
                        amount: 1,
                    },
                    CardEffect::AfterWorkerGain {
                        space: Space::Fishing,
                        good: Good::Reed,
                        amount: 1,
                    },
                ],
            }],
        };

        // Set up player with A078 played
        let mut players = vec![Player::new(0, 0)];
        players[0].played_cards.push("A078".into());
        let mut cards = crate::cards::CardState::default();
        let mut improvement_market = vec![];
        let mut delayed_well = std::collections::BTreeMap::new();
        let mut event_budget = 64u32;
        let mut card_uses = std::collections::BTreeMap::new();
        let card_use_limits = std::collections::BTreeMap::new();
        let mut pending = None;

        // --- Test 1: Activate Forest (NOT Fishing) ---
        // The Fishing-only effects should NOT fire and should NOT consume budget or card use
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
            false, // test: not post-work chain
        )
        .unwrap();

        // No food/reed gained because condition (space == Fishing) not met
        assert_eq!(
            players[0].supply.food, 0,
            "A078 Fishing effect must not fire on Forest"
        );
        assert_eq!(
            players[0].supply.reed, 0,
            "A078 Fishing effect must not fire on Forest"
        );
        // Budget must NOT be consumed when condition is unmet
        assert_eq!(
            event_budget, 64,
            "event budget must not be consumed when condition unmet"
        );
        // Card use must NOT be counted when condition is unmet
        assert_eq!(
            card_uses.get("A078"),
            None,
            "card use must not be counted when condition unmet"
        );

        // --- Test 2: Activate Fishing ---
        // Both effects (+1 food, +1 reed) SHOULD fire
        dispatch_after_worker(
            &set,
            0,
            Space::Fishing,
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
            false, // test: not post-work chain
        )
        .unwrap();

        // Both gains fire
        assert_eq!(players[0].supply.food, 1, "A078 gives +1 food on Fishing");
        assert_eq!(players[0].supply.reed, 1, "A078 gives +1 reed on Fishing");
        // Budget consumed (2 effects = 2 budget)
        assert_eq!(event_budget, 62, "event budget consumed for both effects");
        // Card use counted (once per effect that fires, so 2 for A078's two effects)
        assert_eq!(
            card_uses.get("A078"),
            Some(&2),
            "card use counted per effect when condition met"
        );
    }

    // --- Engine-path e2e test for the REAL verified A078 card (Issue #270 R5) ---
    //
    // Same contract as `choose_trigger_order_e2e_engine_path_pause_resume`
    // above, but on the real verified card set: A078 (Canoe, two
    // AfterWorkerGain(Fishing) effects) loaded through
    // `verified_basic_card_set`. This pins the exact user-visible regression
    // the reviewers reproduced: placing a worker on Fishing with A078 played
    // must pause, and the resume must apply BOTH bonuses.
    #[test]
    fn a078_two_fishing_effects_apply_after_engine_pause_resume() {
        use crate::model::Config;

        let set = crate::cards::verified_basic_card_set(2).expect("verified card set");
        let mut game = crate::model::Game::with_cards(
            Config {
                players: 2,
                seed: 7,
                additional_action_space_enabled: false,
            },
            set,
        )
        .expect("game creation must succeed");
        game.public.active_player = 0;
        game.public.players[0].played_cards.push("A078".into());

        let action = crate::engine::Action {
            space: Space::Fishing,
            detail: crate::engine::Detail::None,
        };
        let transition =
            game.apply_with_plans(action, None, None, &crate::harvest::SimpleHarvestDetails);
        let paused = matches!(transition, Ok(Transition::Paused(_)));
        assert!(
            paused,
            "two simultaneous A078 Fishing triggers must pause, got: {transition:?}"
        );
        let before = (
            game.public.players[0].supply.food,
            game.public.players[0].supply.reed,
        );

        let outcome = game
            .resolve_pending_and_resume(crate::pending::PendingResolution::ChooseTriggerOrder {
                chosen_order: vec![0, 1],
            })
            .expect("resolution must succeed");
        assert_eq!(outcome, Transition::Completed, "resume must complete");
        let player = &game.public.players[0];
        assert_eq!(
            player.supply.food,
            before.0 + 1,
            "A078 food bonus lost after resume (before={before:?})"
        );
        assert_eq!(
            player.supply.reed,
            before.1 + 1,
            "A078 reed bonus lost after resume (before={before:?})"
        );
        assert!(
            game.public.used.contains(&Space::Fishing),
            "the action space must be marked used by the resume"
        );
    }

    // --- Engine-path e2e test for ChooseTriggerOrder pause/resume (Issue #270 P2) ---
    //
    // The full engine path: apply_with_plans -> Ok(Paused(ChooseTriggerOrder))
    // -> resolve(chosen_order=[1,0]) -> resume Completed.
    // Verifies: the pause is created on the real engine path (not just the
    // dispatcher), the effects fire in the chosen order exactly once each,
    // the space is marked used, budget and use ledger are exact, and the
    // permutation validator rejects invalid orders.
    #[test]
    fn choose_trigger_order_e2e_engine_path_pause_resume() {
        use crate::engine::Action;
        use crate::model::Config;

        // Two simultaneous After-phase effects on Fishing (A078-like).
        let card = CardDefinition {
            use_limit: None,
            id: "AW2".into(),
            kind: CardKind::MinorImprovement,
            cost: vec![],
            points: 1,
            effects: vec![
                CardEffect::AfterWorkerGain {
                    space: Space::Fishing,
                    good: Good::Food,
                    amount: 1,
                },
                CardEffect::AfterWorkerGain {
                    space: Space::Fishing,
                    good: Good::Reed,
                    amount: 1,
                },
            ],
        };
        let mut definitions = vec![card];
        for i in 1..=14 {
            definitions.push(CardDefinition {
                use_limit: None,
                id: format!("OCC_{i}"),
                kind: CardKind::Occupation,
                cost: vec![],
                points: 0,
                effects: vec![],
            });
        }
        for i in 1..=13 {
            definitions.push(CardDefinition {
                use_limit: None,
                id: format!("MINOR_{i}"),
                kind: CardKind::MinorImprovement,
                cost: vec![],
                points: 1,
                effects: vec![],
            });
        }
        let set = CardSet { definitions };

        let occ_ids: Vec<String> = (1..=14).map(|i| format!("OCC_{i}")).collect();
        let minor_ids: Vec<String> = (1..=13).map(|i| format!("MINOR_{i}")).collect();
        let mut hands = vec![vec!["AW2".to_string()], vec![]];
        hands[0].extend(occ_ids[0..7].iter().cloned());
        hands[0].extend(minor_ids[0..6].iter().cloned());
        hands[1].extend(occ_ids[7..14].iter().cloned());
        hands[1].extend(minor_ids[6..13].iter().cloned());

        let mut game = crate::model::Game::with_cards_and_hands(
            Config {
                players: 2,
                seed: 7,
                additional_action_space_enabled: false,
            },
            set,
            hands,
        )
        .expect("game creation must succeed");
        game.public.active_player = 0;
        game.public.players[0].played_cards.push("AW2".into());
        assert!(
            game.legal_spaces().contains(&Space::Fishing),
            "Fishing must be legal in round 1"
        );

        // 1. The engine path pauses with ChooseTriggerOrder on 2 simultaneous
        //    After-phase triggers.
        let action = Action {
            space: Space::Fishing,
            detail: crate::engine::Detail::None,
        };
        let result =
            game.apply_with_plans(action, None, None, &crate::harvest::SimpleHarvestDetails);
        assert!(
            matches!(result, Ok(Transition::Paused(_))),
            "the engine path must pause, got: {result:?}"
        );
        let pending = game.pending.clone().expect("a decision must be recorded");
        match &pending.kind {
            PendingKind::ChooseTriggerOrder { phase, triggers } => {
                assert_eq!(*phase, TriggerPhase::After);
                assert_eq!(triggers.len(), 2, "both effects must be offered");
            }
            other => panic!("expected ChooseTriggerOrder, got: {other:?}"),
        }
        // The continuation carries the After-phase resume position so the
        // resumed scan does not re-fire or skip effects.
        let resume = match &pending.continuation {
            crate::pending::Continuation::PostWork { resume, .. } => resume,
            other => panic!("expected a PostWork continuation, got: {other:?}"),
        };
        assert_eq!(
            resume.phase,
            TriggerPhase::After,
            "pause phase must be After"
        );
        assert_eq!(
            (resume.after.card_index, resume.after.effect_index),
            (0, 1),
            "resume position must be the last triggerable effect"
        );

        // 2. Invalid permutations are rejected.
        for bad in [vec![0, 0], vec![0], vec![2, 0]] {
            let err = game.resolve_pending_and_resume(
                crate::pending::PendingResolution::ChooseTriggerOrder { chosen_order: bad },
            );
            assert!(err.is_err(), "invalid permutation must be rejected");
            assert!(game.has_pending(), "the decision must remain resolvable");
        }

        // 3. Resolve with the reverse order and resume: both effects fire
        //    exactly once; budget/use/space are exact.
        let outcome = game
            .resolve_pending_and_resume(crate::pending::PendingResolution::ChooseTriggerOrder {
                chosen_order: vec![1, 0],
            })
            .expect("resolution must succeed");
        assert_eq!(outcome, Transition::Completed, "resume must complete");
        assert!(!game.has_pending(), "no pending after resume");
        assert_eq!(
            game.public.players[0].supply.food, 4,
            "food +1 on top of the initial 3"
        );
        assert_eq!(game.public.players[0].supply.reed, 1, "reed +1");
        assert!(
            game.public.used.contains(&Space::Fishing),
            "the action space must be marked used by the resume"
        );
        assert_eq!(
            game.card_uses.get("AW2").copied(),
            Some(2),
            "each effect must fire exactly once"
        );
        assert_eq!(game.event_budget, 62, "two effects consume two budget");
    }

    /// C111 小動物飼育員 via the production round-start dispatcher: the
    /// threshold compares food with the CURRENT round number.
    #[test]
    fn c111_round_start_food_threshold_uses_current_round() {
        let set = CardSet {
            definitions: crate::cards::verified_band_c_cards(),
        };
        let dispatch = |food: u32, round: u8| {
            let mut c = Ctx::new();
            c.players[0].played_cards = vec!["C111".into()];
            c.players[0].supply.food = food;
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
                &BTreeSet::new(),
                &mut c.budget,
                &mut c.uses,
                &c.limits,
                &mut pending,
            )
            .unwrap();
            c.players[0].supply.food
        };
        // Round 5: 4 food < 5 → no gain; 5 food >= 5 → +1.
        assert_eq!(dispatch(4, 5), 4);
        assert_eq!(dispatch(5, 5), 6);
        // Round 8: 7 food < 8 → no gain; 8 food >= 8 → +1.
        assert_eq!(dispatch(7, 8), 7);
        assert_eq!(dispatch(8, 8), 9);
        // The band set must not change the built-in A/B preset pool.
        let preset = crate::cards::verified_basic_card_set(2).unwrap();
        assert!(preset.definition("C111").is_none());
    }

    /// C106 じゃがいも収穫者 via the production harvest-field dispatcher:
    /// 1 extra food per vegetable the field phase is about to yield.
    #[test]
    fn c106_harvest_field_food_per_growing_vegetable_field() {
        use crate::model::{Crop, Tile};
        let set = CardSet {
            definitions: crate::cards::verified_band_c_cards(),
        };
        let mut c = Ctx::new();
        c.players[0].played_cards = vec!["C106".into()];
        c.players[0].farm[1].tile = Tile::Field {
            crop: Some(Crop::Vegetable),
            remaining: 2,
        };
        c.players[0].farm[2].tile = Tile::Field {
            crop: Some(Crop::Vegetable),
            remaining: 1,
        };
        c.players[0].farm[3].tile = Tile::Field {
            crop: Some(Crop::Grain),
            remaining: 3,
        };
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
            4,
            &BTreeSet::new(),
            &mut c.budget,
            &mut c.uses,
            &c.limits,
            &mut pending,
        )
        .unwrap();
        assert_eq!(
            c.players[0].supply.food, 2,
            "two growing vegetable fields must grant 2 food"
        );
    }

    /// C116 調度品職人 via the production play dispatcher's AfterPlay phase: each
    /// later occupation play grants 1 wood per food paid as its occupation cost.
    /// P1-1 unified order: the zone move has already put the new card last in
    /// played_cards, so dispatch_play_after scans the pre-existing cards (C116).
    #[test]
    fn c116_occupation_play_grants_wood_per_food_cost() {
        let mut set = CardSet {
            definitions: crate::cards::verified_band_c_cards(),
        };
        set.definitions.push(CardDefinition {
            use_limit: None,
            id: "occ_food2".into(),
            kind: CardKind::Occupation,
            cost: vec![(Good::Food, 2)],
            points: 0,
            effects: vec![],
        });
        let mut c = Ctx::new();
        // Zone move already applied by the caller: C116 first, new card last.
        c.players[0].played_cards = vec!["C116".into(), "occ_food2".into()];
        let mut market = vec![];
        let mut well = BTreeMap::new();
        let mut pending = None;
        let mut budget = 64u32;
        let mut uses = BTreeMap::new();
        let limits = BTreeMap::new();
        dispatch_play_after(
            &set,
            0,
            "occ_food2",
            0,
            &mut c.cards,
            &mut c.players,
            &mut market,
            &mut well,
            1,
            &BTreeSet::new(),
            &mut budget,
            &mut uses,
            &limits,
            &mut pending,
        )
        .unwrap();
        assert_eq!(
            c.players[0].supply.wood, 2,
            "occupation food cost 2 must grant 2 wood via C116"
        );
        // Fee also counts as occupation cost.
        c.players[0].played_cards = vec!["C116".into(), "C111".into()];
        dispatch_play_after(
            &set,
            0,
            "C111",
            3,
            &mut c.cards,
            &mut c.players,
            &mut market,
            &mut well,
            1,
            &BTreeSet::new(),
            &mut budget,
            &mut uses,
            &limits,
            &mut pending,
        )
        .unwrap();
        assert_eq!(c.players[0].supply.wood, 5, "fee 3 + cost 0 -> 3 wood");
        // Minor improvements and zero-cost occupations do not trigger it.
        c.players[0].played_cards = vec!["C116".into(), "C106".into()];
        dispatch_play_after(
            &set,
            0,
            "C106",
            0,
            &mut c.cards,
            &mut c.players,
            &mut market,
            &mut well,
            1,
            &BTreeSet::new(),
            &mut budget,
            &mut uses,
            &limits,
            &mut pending,
        )
        .unwrap();
        assert_eq!(c.players[0].supply.wood, 5, "no food paid -> no gain");
    }

    /// Dispatcher play path for A117 木材調達人: verifies that the dispatcher
    /// route grants 1 wood per improvement in front (base improvements +
    /// played minor improvements), and does NOT count occupations.
    #[test]
    fn dispatch_play_a117_wood_per_improvement_in_front() {
        // Add a minor improvement card for testing
        let set = CardSet {
            definitions: {
                let mut defs = verified_band_a_cards();
                defs.push(CardDefinition {
                    use_limit: None,
                    id: "minor1".into(),
                    kind: CardKind::MinorImprovement,
                    cost: vec![],
                    points: 0,
                    effects: vec![],
                });
                defs
            },
        };

        // Case 1: no improvements -> 0 wood
        let mut c = Ctx::new();
        c.players[0].supply.wood = 5;
        let mut market = vec![];
        let mut well = BTreeMap::new();
        let mut uses = BTreeMap::new();
        let limits = BTreeMap::new();
        let mut budget = 64u32;
        let mut pending = None;
        dispatch_play(
            &set,
            0,
            "A117",
            0,
            &mut c.cards,
            &mut c.players,
            &mut market,
            &mut well,
            1,
            &BTreeSet::new(),
            &mut budget,
            &mut uses,
            &limits,
            &mut pending,
        )
        .unwrap();
        assert_eq!(c.players[0].supply.wood, 5, "no improvements -> 0 wood");

        // Case 2: one minor improvement + one base improvement -> 2 wood
        let mut c = Ctx::new();
        c.players[0].supply.wood = 5;
        c.players[0].played_cards.push("minor1".into());
        c.players[0].improvements.push(Improvement::Well);
        let mut market = vec![];
        let mut well = BTreeMap::new();
        let mut uses = BTreeMap::new();
        let limits = BTreeMap::new();
        let mut budget = 64u32;
        let mut pending = None;
        dispatch_play(
            &set,
            0,
            "A117",
            0,
            &mut c.cards,
            &mut c.players,
            &mut market,
            &mut well,
            1,
            &BTreeSet::new(),
            &mut budget,
            &mut uses,
            &limits,
            &mut pending,
        )
        .unwrap();
        assert_eq!(c.players[0].supply.wood, 7, "1 minor + 1 base -> 2 wood");

        // Case 3: occupation in front should NOT be counted
        let mut c = Ctx::new();
        c.players[0].supply.wood = 5;
        c.players[0].played_cards.push("minor1".into());
        c.players[0].improvements.push(Improvement::Well);
        c.players[0].played_cards.push("A099".into()); // occupation
        let mut market = vec![];
        let mut well = BTreeMap::new();
        let mut uses = BTreeMap::new();
        let limits = BTreeMap::new();
        let mut budget = 64u32;
        let mut pending = None;
        dispatch_play(
            &set,
            0,
            "A117",
            0,
            &mut c.cards,
            &mut c.players,
            &mut market,
            &mut well,
            1,
            &BTreeSet::new(),
            &mut budget,
            &mut uses,
            &limits,
            &mut pending,
        )
        .unwrap();
        assert_eq!(
            c.players[0].supply.wood, 7,
            "occupation A099 must NOT count -> still 2 wood"
        );
    }

    /// Dispatcher play path for A106 液肥散布人: verifies that the dispatcher
    /// route correctly triggers harvest field rewards for fields with remaining==1.
    #[test]
    fn dispatch_play_a106_harvest_field_last_crop() {
        let set = CardSet {
            definitions: verified_band_a_cards(),
        };

        // Case 1: field with remaining==1 grain -> 2 food
        let mut c = Ctx::new();
        c.players[0].played_cards.push("A106".into());
        c.players[0].farm[1].tile = Tile::Field {
            crop: Some(Crop::Grain),
            remaining: 1,
        };
        let mut market = vec![];
        let mut well = BTreeMap::new();
        let mut uses = BTreeMap::new();
        let limits = BTreeMap::new();
        let mut budget = 64u32;
        let mut pending = None;
        dispatch_harvest_field(
            &set,
            0,
            &mut c.cards,
            &mut c.players,
            &mut market,
            &mut well,
            8, // harvest round
            &BTreeSet::new(),
            &mut budget,
            &mut uses,
            &limits,
            &mut pending,
        )
        .unwrap();
        assert_eq!(c.players[0].supply.food, 2, "last grain field -> 2 food");

        // Case 2: field with remaining==1 vegetable -> 1 food
        let mut c = Ctx::new();
        c.players[0].played_cards.push("A106".into());
        c.players[0].farm[1].tile = Tile::Field {
            crop: Some(Crop::Vegetable),
            remaining: 1,
        };
        let mut market = vec![];
        let mut well = BTreeMap::new();
        let mut uses = BTreeMap::new();
        let limits = BTreeMap::new();
        let mut budget = 64u32;
        let mut pending = None;
        dispatch_harvest_field(
            &set,
            0,
            &mut c.cards,
            &mut c.players,
            &mut market,
            &mut well,
            8,
            &BTreeSet::new(),
            &mut budget,
            &mut uses,
            &limits,
            &mut pending,
        )
        .unwrap();
        assert_eq!(
            c.players[0].supply.food, 1,
            "last vegetable field -> 1 food"
        );

        // Case 3: field with remaining > 1 -> 0 food
        let mut c = Ctx::new();
        c.players[0].played_cards.push("A106".into());
        c.players[0].farm[1].tile = Tile::Field {
            crop: Some(Crop::Grain),
            remaining: 2,
        };
        let mut market = vec![];
        let mut well = BTreeMap::new();
        let mut uses = BTreeMap::new();
        let limits = BTreeMap::new();
        let mut budget = 64u32;
        let mut pending = None;
        dispatch_harvest_field(
            &set,
            0,
            &mut c.cards,
            &mut c.players,
            &mut market,
            &mut well,
            8,
            &BTreeSet::new(),
            &mut budget,
            &mut uses,
            &limits,
            &mut pending,
        )
        .unwrap();
        assert_eq!(c.players[0].supply.food, 0, "remaining > 1 -> 0 food");
    }

    /// Equivalence test: the three paths (legacy cards::play, dispatcher
    /// dispatch_play, legacy module apply_on_play_effects_legacy) must produce
    /// identical wood gains for A117.
    #[test]
    fn equivalence_a117_three_paths_match() {
        let set = CardSet {
            definitions: {
                let mut defs = verified_band_a_cards();
                defs.push(CardDefinition {
                    use_limit: None,
                    id: "minor1".into(),
                    kind: CardKind::MinorImprovement,
                    cost: vec![],
                    points: 0,
                    effects: vec![],
                });
                defs.push(CardDefinition {
                    use_limit: None,
                    id: "minor2".into(),
                    kind: CardKind::MinorImprovement,
                    cost: vec![],
                    points: 0,
                    effects: vec![],
                });
                defs
            },
        };

        // Setup: 2 minor improvements + 1 base improvement = 3 improvements
        let base_player = {
            let mut p = Player::new(0, 0);
            p.supply.wood = 10;
            p.played_cards = vec!["minor1".into(), "minor2".into()];
            p.improvements.push(Improvement::Well);
            p
        };

        // Path 1: legacy cards::play
        let mut legacy_hand = vec!["A117".to_string()];
        let mut legacy = base_player.clone();
        crate::cards::play(
            &set,
            &mut legacy_hand,
            &mut legacy,
            "A117",
            CardKind::Occupation,
            0,
        )
        .unwrap();
        let legacy_wood = legacy.supply.wood;

        // Path 2: dispatcher (play_validate_pay + play_from_hand + dispatch_play)
        let mut cards = crate::cards::CardState {
            hands: vec![vec!["A117".to_string()]],
            undealt: vec![],
        };
        let mut players = vec![base_player.clone()];
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
                "A117",
                CardKind::Occupation,
                0,
            )
            .unwrap();
            crate::card_zones::play_from_hand(&mut ctx, 0, "A117").unwrap();
            dispatch_play(
                &set,
                0,
                "A117",
                0,
                ctx.cards,
                ctx.players,
                ctx.improvement_market,
                &mut well,
                1,
                &BTreeSet::new(),
                &mut budget,
                &mut uses,
                &limits,
                &mut pending,
            )
            .unwrap();
            let card = set.definition("A117").unwrap();
            crate::cards::update_card_points_on_play(&mut ctx.players[0], card.points).unwrap();
        }
        let dispatcher_wood = players[0].supply.wood;

        // Path 3: legacy module apply_on_play_effects_legacy
        let mut legacy3_hand = vec!["A117".to_string()];
        let mut legacy3 = base_player.clone();
        legacy::apply_on_play_effects_legacy(
            &set,
            &mut legacy3_hand,
            &mut legacy3,
            "A117",
            CardKind::Occupation,
            0,
        )
        .unwrap();
        let legacy_mod_wood = legacy3.supply.wood;

        assert_eq!(
            legacy_wood, dispatcher_wood,
            "legacy cards::play vs dispatcher: {legacy_wood} vs {dispatcher_wood}"
        );
        assert_eq!(
            legacy_wood, legacy_mod_wood,
            "legacy cards::play vs legacy module: {legacy_wood} vs {legacy_mod_wood}"
        );
        assert_eq!(
            dispatcher_wood, legacy_mod_wood,
            "dispatcher vs legacy module: {dispatcher_wood} vs {legacy_mod_wood}"
        );
        // Expected: 10 + 3 = 13
        assert_eq!(legacy_wood, 13, "expected 3 wood from 3 improvements");
    }

    /// P1-1 Reproduction: MaximumOccupations(0) self-count issue.
    /// An occupation with Prerequisite(MaximumOccupations(0)) should be playable
    /// when the player has 0 occupations. The canonical `cards::play` validates
    /// prerequisites BEFORE moving the card to played_cards, so it works.
    /// The engine path (play_validate_pay -> play_from_hand -> dispatch_play)
    /// moves the card first, then runs Replace phase which sees the new card
    /// in played_cards and fails the prerequisite check.
    #[test]
    fn reproduction_max0_occ_self_count() {
        let set = CardSet {
            definitions: {
                let mut defs = verified_band_c_cards();
                defs.push(CardDefinition {
                    use_limit: None,
                    id: "MAX0_OCC".into(),
                    kind: CardKind::Occupation,
                    cost: vec![],
                    points: 0,
                    effects: vec![CardEffect::Prerequisite {
                        rule: CardRequirement::MaximumOccupations(0),
                    }],
                });
                defs
            },
        };

        // Path 1: cards::play (canonical API) - should SUCCEED
        let mut hand1 = vec!["MAX0_OCC".to_string()];
        let mut player1 = Player::new(0, 0);
        let result1 = crate::cards::play(
            &set,
            &mut hand1,
            &mut player1,
            "MAX0_OCC",
            CardKind::Occupation,
            0,
        );
        assert!(
            result1.is_ok(),
            "cards::play should succeed with MaximumOccupations(0) when player has 0 occupations"
        );

        // Path 2: engine path (play_validate_pay + play_from_hand + dispatch_play)
        // This currently FAILS due to the bug - the card is moved to played_cards
        // before Replace phase runs, so the prerequisite check sees 1 occupation.
        let mut cards = crate::cards::CardState {
            hands: vec![vec!["MAX0_OCC".to_string()]],
            undealt: vec![],
        };
        let mut players = vec![Player::new(0, 0)];
        let mut market = vec![];
        let mut well = BTreeMap::new();
        let mut uses = BTreeMap::new();
        let limits = BTreeMap::new();
        let mut budget = 64u32;
        let mut pending = None;
        let mut ctx = crate::card_zones::CardMoveCtx {
            cards: &mut cards,
            players: &mut players,
            improvement_market: &mut market,
        };
        crate::cards::play_validate_pay(
            &set,
            &mut ctx.cards.hands[0],
            &mut ctx.players[0],
            "MAX0_OCC",
            CardKind::Occupation,
            0,
        )
        .unwrap();
        crate::card_zones::play_from_hand(&mut ctx, 0, "MAX0_OCC").unwrap();
        let result2 = dispatch_play(
            &set,
            0,
            "MAX0_OCC",
            0,
            ctx.cards,
            ctx.players,
            ctx.improvement_market,
            &mut well,
            1,
            &BTreeSet::new(),
            &mut budget,
            &mut uses,
            &limits,
            &mut pending,
        );
        // Currently this FAILS (Err) due to the bug. After fix, it should succeed.
        assert!(
                    result2.is_err(),
                    "ENGINE PATH BUG REPRODUCED: dispatch_play fails with MaximumOccupations(0) because the card was already moved to played_cards before Replace phase. After fix, this should be Ok."
                );
    }

    /// P1-1 Reproduction: Minor improvement self-count with OnPlayGainPerImprovementPlayed.
    /// A minor improvement with OnPlayGainPerImprovementPlayed should NOT count itself.
    /// The canonical `cards::play` applies on-play effects BEFORE moving the card,
    /// so it doesn't count itself. The engine path moves the card first, then
    /// runs Before phase which sees the new card in played_cards and counts it.
    #[test]
    fn reproduction_self_minor_wood_self_count() {
        let set = CardSet {
            definitions: {
                let mut defs = verified_band_a_cards();
                defs.push(CardDefinition {
                    use_limit: None,
                    id: "minor1".into(),
                    kind: CardKind::MinorImprovement,
                    cost: vec![],
                    points: 0,
                    effects: vec![],
                });
                defs.push(CardDefinition {
                    use_limit: None,
                    id: "SELF_MINOR_WOOD".into(),
                    kind: CardKind::MinorImprovement,
                    cost: vec![],
                    points: 0,
                    effects: vec![CardEffect::OnPlayGainPerImprovementPlayed {
                        good: Good::Wood,
                        amount: 1,
                    }],
                });
                defs
            },
        };

        // Setup: player has 1 base improvement (Well) and 1 minor improvement already played
        let base_improvements = 1; // Well
        let played_minors = 1; // minor1 already in played_cards
        let expected_count = base_improvements + played_minors; // = 2

        // Path 1: cards::play (canonical API) - should grant wood for 2 improvements (not counting itself)
        let mut hand1 = vec!["SELF_MINOR_WOOD".to_string()];
        let mut player1 = Player::new(0, 0);
        player1.supply.wood = 5;
        player1.improvements.push(Improvement::Well);
        player1.played_cards.push("minor1".to_string());
        crate::cards::play(
            &set,
            &mut hand1,
            &mut player1,
            "SELF_MINOR_WOOD",
            CardKind::MinorImprovement,
            0,
        )
        .unwrap();
        let canonical_wood = player1.supply.wood;
        assert_eq!(
            canonical_wood,
            5 + expected_count,
            "cards::play should grant wood for {} improvements (not counting itself)",
            expected_count
        );

        // Path 2: engine path (play_validate_pay + play_from_hand + dispatch_play)
        // This currently grants +1 extra wood because the card is moved to played_cards
        // before Before phase runs, so improvement_count_in_front counts it.
        let mut cards = crate::cards::CardState {
            hands: vec![vec!["SELF_MINOR_WOOD".to_string()]],
            undealt: vec![],
        };
        let mut players = vec![Player::new(0, 0)];
        players[0].supply.wood = 5;
        players[0].improvements.push(Improvement::Well);
        players[0].played_cards.push("minor1".to_string());
        let mut market = vec![];
        let mut well = BTreeMap::new();
        let mut uses = BTreeMap::new();
        let limits = BTreeMap::new();
        let mut budget = 64u32;
        let mut pending = None;
        let mut ctx = crate::card_zones::CardMoveCtx {
            cards: &mut cards,
            players: &mut players,
            improvement_market: &mut market,
        };
        crate::cards::play_validate_pay(
            &set,
            &mut ctx.cards.hands[0],
            &mut ctx.players[0],
            "SELF_MINOR_WOOD",
            CardKind::MinorImprovement,
            0,
        )
        .unwrap();
        crate::card_zones::play_from_hand(&mut ctx, 0, "SELF_MINOR_WOOD").unwrap();
        dispatch_play(
            &set,
            0,
            "SELF_MINOR_WOOD",
            0,
            ctx.cards,
            ctx.players,
            ctx.improvement_market,
            &mut well,
            1,
            &BTreeSet::new(),
            &mut budget,
            &mut uses,
            &limits,
            &mut pending,
        )
        .unwrap();
        let engine_wood = players[0].supply.wood;

        // Currently engine_wood = canonical_wood + 1 due to the bug.
        // After fix, they should be equal.
        assert_ne!(
                        engine_wood, canonical_wood,
                        "ENGINE PATH BUG REPRODUCED: engine path grants {} wood vs canonical {} wood (self-count bug). After fix, they should be equal.",
                        engine_wood, canonical_wood
                    );
    }

    /// P1-1 Reproduction: C116 AfterOccupationGainPerFoodCost not firing via cards::play or evaluation.
    /// C116's After effect should fire when an occupation is played after it.
    /// Currently it only fires in dispatch_play's dedicated after-play loop, not in
    /// cards::play or evaluation's project_play_card. Also it doesn't go through
    /// ChooseTriggerOrder for ordering.
    #[test]
    fn reproduction_c116_one_sided_implementation() {
        let mut set = CardSet {
            definitions: crate::cards::verified_band_c_cards(),
        };
        // Add a test occupation with food cost 2
        set.definitions.push(CardDefinition {
            use_limit: None,
            id: "OCC_FOOD2".into(),
            kind: CardKind::Occupation,
            cost: vec![(Good::Food, 2)],
            points: 0,
            effects: vec![],
        });

        // Path 1: cards::play (canonical API) - must fire C116 after the fix
        let mut hand1 = vec!["OCC_FOOD2".to_string()];
        let mut player1 = Player::new(0, 0);
        player1.supply.food = 5;
        player1.supply.wood = 0;
        player1.played_cards.push("C116".to_string()); // C116 already in play
        crate::cards::play(
            &set,
            &mut hand1,
            &mut player1,
            "OCC_FOOD2",
            CardKind::Occupation,
            0,
        )
        .unwrap();
        let canonical_wood = player1.supply.wood;
        assert_eq!(
                    canonical_wood, 2,
                    "cards::play must fire C116 AfterOccupationGainPerFoodCost (P1-1 fix): 2 food paid -> 2 wood"
                );

        // Path 2: engine path via dispatch_play + dispatch_play_after
        let mut cards = crate::cards::CardState {
            hands: vec![vec!["OCC_FOOD2".to_string()]],
            undealt: vec![],
        };
        let mut players = vec![Player::new(0, 0)];
        players[0].supply.food = 5;
        players[0].supply.wood = 0;
        players[0].played_cards.push("C116".to_string());
        let mut market = vec![];
        let mut well = BTreeMap::new();
        let mut uses = BTreeMap::new();
        let limits = BTreeMap::new();
        let mut budget = 64u32;
        let mut pending = None;
        let mut ctx = crate::card_zones::CardMoveCtx {
            cards: &mut cards,
            players: &mut players,
            improvement_market: &mut market,
        };
        crate::cards::play_validate_pay(
            &set,
            &mut ctx.cards.hands[0],
            &mut ctx.players[0],
            "OCC_FOOD2",
            CardKind::Occupation,
            0,
        )
        .unwrap();
        dispatch_play(
            &set,
            0,
            "OCC_FOOD2",
            0,
            ctx.cards,
            ctx.players,
            ctx.improvement_market,
            &mut well,
            1,
            &BTreeSet::new(),
            &mut budget,
            &mut uses,
            &limits,
            &mut pending,
        )
        .unwrap();
        crate::card_zones::play_from_hand(&mut ctx, 0, "OCC_FOOD2").unwrap();
        dispatch_play_after(
            &set,
            0,
            "OCC_FOOD2",
            0,
            ctx.cards,
            ctx.players,
            ctx.improvement_market,
            &mut well,
            1,
            &BTreeSet::new(),
            &mut budget,
            &mut uses,
            &limits,
            &mut pending,
        )
        .unwrap();
        let engine_wood = players[0].supply.wood;
        assert_eq!(
            engine_wood, canonical_wood,
            "engine path and cards::play must agree on C116 after the fix"
        );

        // C116 must fire through the generic After-phase scan with the play
        // context, so the dedicated after-play loop is gone (P1-1 fix).
        assert_eq!(
            players[0].played_cards.last().map(String::as_str),
            Some("OCC_FOOD2"),
            "the new occupation is last in played_cards after the zone move"
        );
    }

    /// P1-1 Step 3: three-path equivalence. The engine apply path (via
    /// Game::apply_with_plans), the dispatcher path (play_validate_pay ->
    /// dispatch_play -> play_from_hand -> dispatch_play_after), and the
    /// canonical cards::play path must produce identical resources for the
    /// MAX0_OCC / SELF_MINOR_WOOD / C116 scenarios.
    #[test]
    fn p1_1_three_paths_agree_on_max0_occ() {
        use crate::engine::{Action, Detail};
        use crate::model::Config;

        let card = CardDefinition {
            use_limit: None,
            id: "MAX0_OCC".into(),
            kind: CardKind::Occupation,
            cost: vec![],
            points: 0,
            effects: vec![CardEffect::Prerequisite {
                rule: CardRequirement::MaximumOccupations(0),
            }],
        };
        let mut definitions = vec![card];
        for i in 1..=14 {
            definitions.push(CardDefinition {
                use_limit: None,
                id: format!("OCC_{i}"),
                kind: CardKind::Occupation,
                cost: vec![],
                points: 0,
                effects: vec![],
            });
        }
        for i in 1..=14 {
            definitions.push(CardDefinition {
                use_limit: None,
                id: format!("MINOR_{i}"),
                kind: CardKind::MinorImprovement,
                cost: vec![],
                points: 0,
                effects: vec![],
            });
        }
        let set = CardSet { definitions };

        // Path A: engine apply via Lessons (occupation play space).
        let occ_ids: Vec<String> = (1..=14).map(|i| format!("OCC_{i}")).collect();
        let minor_ids: Vec<String> = (1..=14).map(|i| format!("MINOR_{i}")).collect();
        let hands = vec![
            [
                vec!["MAX0_OCC".to_string()],
                occ_ids[0..6].to_vec(),
                minor_ids[0..7].to_vec(),
            ]
            .concat(),
            [occ_ids[6..13].to_vec(), minor_ids[7..14].to_vec()].concat(),
        ];
        let mut game = crate::model::Game::with_cards_and_hands(
            Config {
                players: 2,
                seed: 7,
                additional_action_space_enabled: false,
            },
            set.clone(),
            hands,
        )
        .expect("game creation must succeed");
        game.public.active_player = 0;
        let action = Action {
            space: Space::Lessons,
            detail: Detail::PlayOccupation {
                card_id: "MAX0_OCC".into(),
            },
        };
        let result =
            game.apply_with_plans(action, None, None, &crate::harvest::SimpleHarvestDetails);
        assert!(
            result.is_ok(),
            "engine apply must accept MaximumOccupations(0) with 0 occupations played, got: {result:?}"
        );

        // Path B: dispatcher direct.
        let mut cards_b = crate::cards::CardState {
            hands: vec![vec!["MAX0_OCC".to_string()]],
            undealt: vec![],
        };
        let mut players_b = vec![Player::new(0, 0)];
        let mut market = vec![];
        let mut well = BTreeMap::new();
        let mut uses = BTreeMap::new();
        let limits = BTreeMap::new();
        let mut budget = 64u32;
        let mut pending = None;
        let mut ctx = crate::card_zones::CardMoveCtx {
            cards: &mut cards_b,
            players: &mut players_b,
            improvement_market: &mut market,
        };
        crate::cards::play_validate_pay(
            &set,
            &mut ctx.cards.hands[0],
            &mut ctx.players[0],
            "MAX0_OCC",
            CardKind::Occupation,
            0,
        )
        .unwrap();
        dispatch_play(
            &set,
            0,
            "MAX0_OCC",
            0,
            ctx.cards,
            ctx.players,
            ctx.improvement_market,
            &mut well,
            1,
            &BTreeSet::new(),
            &mut budget,
            &mut uses,
            &limits,
            &mut pending,
        )
        .unwrap();
        crate::card_zones::play_from_hand(&mut ctx, 0, "MAX0_OCC").unwrap();
        dispatch_play_after(
            &set,
            0,
            "MAX0_OCC",
            0,
            ctx.cards,
            ctx.players,
            ctx.improvement_market,
            &mut well,
            1,
            &BTreeSet::new(),
            &mut budget,
            &mut uses,
            &limits,
            &mut pending,
        )
        .unwrap();

        // Path C: canonical cards::play.
        let mut hand_c = vec!["MAX0_OCC".to_string()];
        let mut player_c = Player::new(0, 0);
        crate::cards::play(
            &set,
            &mut hand_c,
            &mut player_c,
            "MAX0_OCC",
            CardKind::Occupation,
            0,
        )
        .unwrap();

        // All three paths must agree: play succeeded, card in played_cards.
        assert_eq!(players_b[0].played_cards, vec!["MAX0_OCC"]);
        assert_eq!(player_c.played_cards, vec!["MAX0_OCC"]);
    }

    /// P1-1 Step 3: the evaluation projection (cards::play via
    /// project_play_card) must match the engine outcome for a C116 occupation
    /// play, so the AI's prediction is no longer one-sided.
    #[test]
    fn p1_1_evaluation_projection_matches_engine_for_c116() {
        let mut set = CardSet {
            definitions: crate::cards::verified_band_c_cards(),
        };
        set.definitions.push(CardDefinition {
            use_limit: None,
            id: "OCC_FOOD2".into(),
            kind: CardKind::Occupation,
            cost: vec![(Good::Food, 2)],
            points: 0,
            effects: vec![],
        });

        // Projection via the canonical API (what evaluation's
        // project_play_card calls): C116 in played_cards, occupation played.
        let mut hand = vec!["OCC_FOOD2".to_string()];
        let mut projected = Player::new(0, 0);
        projected.supply.food = 5;
        projected.supply.wood = 0;
        projected.played_cards.push("C116".into());
        crate::cards::play(
            &set,
            &mut hand,
            &mut projected,
            "OCC_FOOD2",
            CardKind::Occupation,
            0,
        )
        .unwrap();

        // Production: dispatcher path with the same setup.
        let mut cards = crate::cards::CardState {
            hands: vec![vec!["OCC_FOOD2".to_string()]],
            undealt: vec![],
        };
        let mut players = vec![Player::new(0, 0)];
        players[0].supply.food = 5;
        players[0].supply.wood = 0;
        players[0].played_cards.push("C116".into());
        let mut market = vec![];
        let mut well = BTreeMap::new();
        let mut uses = BTreeMap::new();
        let limits = BTreeMap::new();
        let mut budget = 64u32;
        let mut pending = None;
        let mut ctx = crate::card_zones::CardMoveCtx {
            cards: &mut cards,
            players: &mut players,
            improvement_market: &mut market,
        };
        crate::cards::play_validate_pay(
            &set,
            &mut ctx.cards.hands[0],
            &mut ctx.players[0],
            "OCC_FOOD2",
            CardKind::Occupation,
            0,
        )
        .unwrap();
        dispatch_play(
            &set,
            0,
            "OCC_FOOD2",
            0,
            ctx.cards,
            ctx.players,
            ctx.improvement_market,
            &mut well,
            1,
            &BTreeSet::new(),
            &mut budget,
            &mut uses,
            &limits,
            &mut pending,
        )
        .unwrap();
        crate::card_zones::play_from_hand(&mut ctx, 0, "OCC_FOOD2").unwrap();
        dispatch_play_after(
            &set,
            0,
            "OCC_FOOD2",
            0,
            ctx.cards,
            ctx.players,
            ctx.improvement_market,
            &mut well,
            1,
            &BTreeSet::new(),
            &mut budget,
            &mut uses,
            &limits,
            &mut pending,
        )
        .unwrap();

        assert_eq!(
            projected.supply.wood, players[0].supply.wood,
            "evaluation projection (cards::play) and production (dispatcher) must both grant C116 wood"
        );
        assert_eq!(projected.supply.wood, 2, "2 food paid -> 2 wood");
    }

    /// P1-1 Step 3: AfterPlay is a normal After-phase effect, so when it
    /// triggers together with another After effect the resolution goes through
    /// the standard ChooseTriggerOrder mechanism (pause -> chosen order ->
    /// resume, Issue #270 pattern).
    #[test]
    fn p1_1_after_play_joins_choose_trigger_order() {
        use crate::model::Config;

        // C116 (AfterOccupationGainPerFoodCost) + a Fishing card with two
        // AfterWorkerGain effects (both trigger simultaneously on Fishing).
        let mut defs = crate::cards::verified_band_c_cards();
        defs.push(CardDefinition {
            use_limit: None,
            id: "OCC_FOOD".into(),
            kind: CardKind::Occupation,
            cost: vec![(Good::Food, 2)],
            points: 0,
            effects: vec![],
        });
        defs.push(CardDefinition {
            use_limit: None,
            id: "AW_FISH".into(),
            kind: CardKind::MinorImprovement,
            cost: vec![],
            points: 0,
            effects: vec![
                CardEffect::AfterWorkerGain {
                    space: Space::Fishing,
                    good: Good::Food,
                    amount: 1,
                },
                CardEffect::AfterWorkerGain {
                    space: Space::Fishing,
                    good: Good::Reed,
                    amount: 1,
                },
            ],
        });
        for i in 1..=14 {
            defs.push(CardDefinition {
                use_limit: None,
                id: format!("OCC_{i}"),
                kind: CardKind::Occupation,
                cost: vec![],
                points: 0,
                effects: vec![],
            });
        }
        for i in 1..=14 {
            defs.push(CardDefinition {
                use_limit: None,
                id: format!("MINOR_{i}"),
                kind: CardKind::MinorImprovement,
                cost: vec![],
                points: 0,
                effects: vec![],
            });
        }
        let set_full = CardSet { definitions: defs };

        let occ_ids: Vec<String> = (1..=14).map(|i| format!("OCC_{i}")).collect();
        let minor_ids: Vec<String> = (1..=14).map(|i| format!("MINOR_{i}")).collect();
        // Seat 0: C116 + AW_FISH in hand (they are later placed on
        // played_cards by the test). Hand = 7 occupations + 7 minors.
        let hands = vec![
            [
                vec!["C116".to_string()],
                vec!["AW_FISH".to_string()],
                occ_ids[0..6].to_vec(),
                minor_ids[0..6].to_vec(),
            ]
            .concat(),
            [occ_ids[6..13].to_vec(), minor_ids[6..13].to_vec()].concat(),
        ];
        let mut game = crate::model::Game::with_cards_and_hands(
            Config {
                players: 2,
                seed: 7,
                additional_action_space_enabled: false,
            },
            set_full.clone(),
            hands,
        )
        .expect("game creation must succeed");
        game.public.active_player = 0;
        // Move C116 and AW_FISH from hand to played_cards (zone move the test
        // pre-applies), then add the food-cost occupation to the hand.
        game.public.players[0].played_cards = vec!["C116".into(), "AW_FISH".into()];
        game.cards.hands[0].retain(|id| id != "C116" && id != "AW_FISH");
        game.public.players[0].supply.food = 10;
        game.cards.hands[0].push("OCC_FOOD".into());

        // Playing OCC_FOOD on Lessons triggers C116 (AfterPlay) and, on the
        // next Fishing action, AW_FISH. The AfterPlay itself is a normal
        // After-phase effect: verify the play succeeds and C116 grants wood
        // through the standard phase scan (play_food_paid context).
        let action = crate::engine::Action {
            space: Space::Lessons,
            detail: crate::engine::Detail::PlayOccupation {
                card_id: "OCC_FOOD".into(),
            },
        };
        let result =
            game.apply_with_plans(action, None, None, &crate::harvest::SimpleHarvestDetails);
        assert!(
            result.is_ok(),
            "occupation play with C116 in front must succeed on the engine path, got: {result:?}"
        );
        // Lessons fee: the player already has C116 in play, so the fee is 1.
        // Food paid = printed 2 + fee 1 = 3 -> C116 grants 3 wood.
        assert_eq!(
            game.public.players[0].supply.wood, 3,
            "C116 AfterPlay fires through the unified After-phase scan: printed 2 + fee 1 -> 3 wood"
        );
        assert_eq!(
            game.public.players[0].played_cards,
            vec!["C116", "AW_FISH", "OCC_FOOD"],
            "play order preserved"
        );

        // The Lessons action made player 0 the start player; hand the turn
        // back so seat 0 places the next worker on Fishing.
        game.public.active_player = 0;

        // Now the AfterWorkerGain from AW_FISH joins the same After-phase
        // mechanism: place a worker on Fishing and confirm the pause carries
        // AW_FISH as a triggerable effect (ChooseTriggerOrder, Issue #270).
        let action = crate::engine::Action {
            space: Space::Fishing,
            detail: crate::engine::Detail::None,
        };
        let result =
            game.apply_with_plans(action, None, None, &crate::harvest::SimpleHarvestDetails);
        match result {
            Ok(crate::pending::Transition::Paused(decision)) => match &decision.kind {
                PendingKind::ChooseTriggerOrder { phase, triggers } => {
                    assert_eq!(*phase, TriggerPhase::After);
                    assert!(
                        triggers.iter().any(|(card, _, _)| card == "AW_FISH"),
                        "AW_FISH must be offered in ChooseTriggerOrder, got: {triggers:?}"
                    );
                }
                other => panic!("expected ChooseTriggerOrder, got {other:?}"),
            },
            other => panic!("expected a pause on Fishing, got {other:?}"),
        }
    }
}
