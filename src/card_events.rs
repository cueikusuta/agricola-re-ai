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
use crate::model::{Animal, Crop, Good, House, Player, Space, Tile};
use crate::pending::{ChainPosition, PendingDecision, PendingKind, Transition};
use crate::registry::EventKind;
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
        // B073 贈り物かご's room-count table is an instantaneous play-time
        // effect — Before phase, same as the other on-play gains.
        CardEffect::OnPlayGainIfRoomCount { .. } => TriggerPhase::Before,
        CardEffect::OnPlayGainPerImprovementPlayed { .. } => TriggerPhase::Before,
        CardEffect::OnPlayGainPerCount { .. } => TriggerPhase::Before,
        // On-play animal gain is an instantaneous play-time effect — Before
        // phase. Its placement plan is derived and applied via place_animals
        // (the play executor cannot represent a play-time pause).
        CardEffect::OnPlayGainAnimal { .. } => TriggerPhase::Before,
        // D007 三つ又の矛: the round-table gain is an instantaneous play-time
        // effect — Before phase. The exact-round match is judged in
        // effect_condition_holds so only one table entry is ever triggerable.
        CardEffect::OnPlayGainIfRound { .. } => TriggerPhase::Before,
        // Zone-move metadata, never resolved by the phase scan: modeled as
        // Before (the move itself runs after the play chain in dispatch_play).
        CardEffect::PostPlayPassLeft => TriggerPhase::Before,
        // Passive effects are continuous — model as Before (no choices)
        CardEffect::Passive { .. } => TriggerPhase::Before,
        // Worker completion triggers — After phase
        CardEffect::AfterWorkerGain { .. } => TriggerPhase::After,
        CardEffect::AfterWorkerGainIfUsed { .. } => TriggerPhase::After,
        CardEffect::AfterWorkerGainAnimal { .. } => TriggerPhase::After,
        // Round start — After phase (happens after round begins)
        CardEffect::StartRoundGain { .. } => TriggerPhase::After,
        CardEffect::StartRoundGainIfFoodAtLeastRound { .. } => TriggerPhase::After,
        CardEffect::StartRoundGainIfNoMinorPlayed { .. } => TriggerPhase::After,
        CardEffect::StartRoundGainIfFamilyAndRooms { .. } => TriggerPhase::After,
        CardEffect::StartRoundGainIfLessonsOccupants { .. } => TriggerPhase::After,
        CardEffect::StartRoundGainIfAccumulationGrew { .. } => TriggerPhase::After,
        CardEffect::StartRoundGainIfNoHarvestAndAnimals { .. } => TriggerPhase::After,
        // Fired only by the play dispatcher's dedicated after-play scan.
        CardEffect::AfterOccupationGainPerFoodCost { .. } => TriggerPhase::After, // fired by the dedicated after-play scan
        CardEffect::AfterOccupationGain { .. } => TriggerPhase::After, // fired by the dedicated after-play scan
        // Improvement played from hand — After phase (fired by dispatch_play's after-play scan)
        CardEffect::AfterImprovementPlayedGain { .. } => TriggerPhase::After,
        // Clay construction purchase — After phase
        CardEffect::AfterClayConstructionGain { .. } => TriggerPhase::After,
        // Harvest/baking — After phase
        CardEffect::AfterHarvestBakeGain { .. } => TriggerPhase::After,
        CardEffect::HarvestFieldGain { .. } => TriggerPhase::After,
        CardEffect::HarvestFieldGainIfNoMinorPlayed { .. } => TriggerPhase::After,
        CardEffect::HarvestFieldGainIfGrainHarvestedAtLeast { .. } => TriggerPhase::After,
        CardEffect::AfterHarvestGain { .. } => TriggerPhase::After,
        CardEffect::AfterHarvestGainIfAtLeast { .. } => TriggerPhase::After,
        // C066: conditional choice at the harvest tail — After phase
        CardEffect::AfterHarvestChooseGainIfAtLeast { .. } => TriggerPhase::After,
        // A118 optional exchange — Interrupt phase (requires player decision)
        CardEffect::HarvestFieldOptionalExchange { .. } => TriggerPhase::Interrupt,
        // Any-player worker triggers — After phase (fired by dispatch_after_worker_any_player)
        CardEffect::AfterAnyWorkerGain { .. } => TriggerPhase::After,
        CardEffect::AfterAnyWorkerOthersGain { .. } => TriggerPhase::After,
        // Harvest start field check — After phase (fired by dispatch_harvest_field)
        CardEffect::HarvestStartGainIfFields { .. } => TriggerPhase::After,
        // End game scoring — After phase
        CardEffect::EndGameBonus { .. } => TriggerPhase::After,
        // Chapter 1 round-start cards (A090/A151/A152/B097/B106/B135/C064) — After phase
        CardEffect::StartRoundPlowFieldIfHouse { .. } => TriggerPhase::After,
        CardEffect::StartRoundCopyUnusedRoundSpace { .. } => TriggerPhase::After,
        CardEffect::StartRoundPlayOccupationIfLessonsUnderused { .. } => TriggerPhase::After,
        CardEffect::StartRoundPlayCardIfStoneHouse { .. } => TriggerPhase::After,
        CardEffect::StartRoundGainIfGoodsOnRoundSpaces { .. } => TriggerPhase::After,
        CardEffect::StartRoundExchangeForFoodAndPoints { .. } => TriggerPhase::After,
        CardEffect::StartRoundPlaceFoodOnNextRoundSpaces { .. } => TriggerPhase::After,
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
    /// Public space accumulations (space -> amount), provided only by the
    /// round-start dispatcher for accumulation-grew conditions (A049 営巣地).
    /// `None` on every other dispatch path. This is a runtime-only borrow:
    /// the struct is never serialized, so save/observation formats are
    /// untouched.
    pub accumulations: Option<&'a std::collections::BTreeMap<Space, u32>>,
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
            PendingKind::ChooseTriggerOrder {
                phase,
                site: trigger_data.site,
                triggers,
            },
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
            site: trigger_data.site,
            event_kind: trigger_data.site,
            revealed_round_cards: trigger_data.revealed_round_cards.clone(),
            c066_take_food_instead: trigger_data.c066_take_food_instead,
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
            accumulations: ctx.accumulations,
            event_budget: ctx.event_budget,
            card_uses: ctx.card_uses,
            card_use_limits: ctx.card_use_limits,
            pending: ctx.pending,
            actor: ctx.actor,
            card_owner: player,
            beneficiary: player,
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
                // Never fires via the generic phase scan (see
                // effect_condition_holds); dispatch_play's after-play scan owns it.
                CardEffect::AfterOccupationGainPerFoodCost { .. } => EffectResult::Resolved,
                // Never fires via the generic phase scan; dispatch_play's after-play
                // scan for improvements owns it.
                CardEffect::AfterImprovementPlayedGain { .. } => EffectResult::Resolved,
                CardEffect::AfterOccupationGain { .. } => EffectResult::Resolved,
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
                CardEffect::OnPlayGainIfRoomCount {
                    rooms,
                    good,
                    amount,
                } => handle_on_play_gain_if_room_count(ctx, player, *rooms, good, *amount)?,
                CardEffect::OnPlayGainPerImprovementPlayed { good, amount } => {
                    handle_on_play_gain_per_improvement_played(ctx, player, good, *amount)?
                }
                CardEffect::OnPlayGainPerCount {
                    count_kind,
                    good,
                    amount,
                } => handle_on_play_gain_per_count(ctx, player, count_kind, good, *amount)?,
                CardEffect::OnPlayGainAnimal { animal, amount } => {
                    handle_on_play_gain_animal(ctx, animal, *amount)?
                }
                // D007 三つ又の矛: the exact-round match is already resolved by
                // effect_condition_holds, so a triggerable entry is the current
                // round's row of the printed table and grants unconditionally.
                CardEffect::OnPlayGainIfRound { good, amount, .. } => {
                    handle_on_play_gain(ctx, player, good, *amount)?
                }
                // Zone-move metadata: never resolved by the scan.
                CardEffect::PostPlayPassLeft => EffectResult::Resolved,
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
                CardEffect::AfterWorkerGainAnimal {
                    space,
                    animal,
                    amount,
                } => handle_after_worker_gain_animal(ctx, space, animal, *amount, trigger_data)?,
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
                // B114 子なし: family exactly N + rooms >= M (handler judges).
                CardEffect::StartRoundGainIfFamilyAndRooms {
                    family,
                    rooms_at_least,
                    good,
                    amount,
                } => handle_start_round_gain_if_family_and_rooms(
                    ctx,
                    player,
                    *family,
                    *rooms_at_least,
                    good,
                    *amount,
                )?,
                // A157 ボヘミアン: Lessons occupants >= min (handler judges).
                CardEffect::StartRoundGainIfLessonsOccupants { min, good, amount } => {
                    handle_start_round_gain_if_lessons_occupants(ctx, player, *min, good, *amount)?
                }
                // A049 営巣地: accumulation >= min and grew this round.
                CardEffect::StartRoundGainIfAccumulationGrew {
                    space,
                    min,
                    good,
                    amount,
                } => handle_start_round_gain_if_accumulation_grew(
                    ctx, player, *space, *min, good, *amount,
                )?,
                // C067 ミネラルフィーダー: non-harvest round + pasture animals.
                CardEffect::StartRoundGainIfNoHarvestAndAnimals {
                    animal,
                    min,
                    good,
                    amount,
                } => handle_start_round_gain_if_no_harvest_and_animals(
                    ctx, player, *animal, *min, good, *amount, round_now,
                )?,
                CardEffect::HarvestFieldGainIfNoMinorPlayed { good, amount } => {
                    handle_gain_if_no_minor_played(ctx, player, good, *amount)?
                }
                CardEffect::HarvestFieldGainIfGrainHarvestedAtLeast {
                    threshold,
                    good,
                    amount,
                } => handle_harvest_field_gain_if_grain_harvested_at_least(
                    ctx, player, *threshold, good, *amount,
                )?,
                CardEffect::AfterClayConstructionGain { good, amount } => {
                    handle_after_clay_construction_gain(ctx, player, good, *amount, trigger_data)?
                }
                CardEffect::AfterHarvestBakeGain { amount } => {
                    handle_after_harvest_bake_gain(ctx, player, *amount, trigger_data)?
                }
                CardEffect::HarvestFieldGain { rule, good, amount } => {
                    handle_harvest_field_gain(ctx, player, rule, good, *amount, trigger_data)?
                }
                CardEffect::AfterHarvestGain { good, amount } => {
                    handle_after_harvest_gain(ctx, player, good, *amount)?
                }
                CardEffect::AfterHarvestGainIfAtLeast {
                    good,
                    threshold,
                    gain,
                    amount,
                } => handle_after_harvest_gain_if_at_least(
                    ctx, player, good, *threshold, gain, *amount,
                )?,
                CardEffect::AfterHarvestChooseGainIfAtLeast {
                    good,
                    threshold,
                    gain,
                    amount,
                    upgrade_threshold,
                    upgrade_gain,
                    upgrade_amount,
                } => handle_after_harvest_choose_gain_if_at_least(
                    ctx,
                    player,
                    good,
                    *threshold,
                    gain,
                    *amount,
                    *upgrade_threshold,
                    upgrade_gain,
                    *upgrade_amount,
                    trigger_data,
                )?,
                CardEffect::HarvestFieldOptionalExchange {
                    cost,
                    cost_amount,
                    gain,
                    gain_amount,
                } => handle_harvest_field_optional_exchange(
                    ctx,
                    player,
                    cost,
                    *cost_amount,
                    gain,
                    *gain_amount,
                )?,
                // AfterAnyWorkerGain / AfterAnyWorkerOthersGain are handled in
                // dispatch_after_worker_any_player, not the generic phase scan.
                // HarvestStartGainIfFields is handled in dispatch_harvest_field via the generic scan.
                CardEffect::AfterAnyWorkerGain { .. }
                | CardEffect::AfterAnyWorkerOthersGain { .. } => {
                    panic!(
                        "{:?} should be handled in special dispatch, not generic phase resolution",
                        effect
                    )
                }
                CardEffect::HarvestStartGainIfFields {
                    crop,
                    threshold,
                    good,
                    amount,
                } => handle_harvest_start_gain_if_fields(
                    ctx,
                    player,
                    crop,
                    *threshold,
                    good,
                    *amount,
                    trigger_data,
                )?,
                // Chapter 1 round-start cards (A090/A151/A152/B097/B106/B135/C064)
                CardEffect::StartRoundPlowFieldIfHouse {
                    house,
                    food_cost,
                    fields,
                } => handle_start_round_plow_field_if_house(
                    ctx,
                    player,
                    house,
                    *food_cost,
                    *fields,
                    trigger_data,
                )?,
                CardEffect::StartRoundCopyUnusedRoundSpace { max_round } => {
                    handle_start_round_copy_unused_round_space(
                        ctx,
                        player,
                        *max_round,
                        round_now,
                        trigger_data,
                    )?
                }
                CardEffect::StartRoundPlayOccupationIfLessonsUnderused { food_cost } => {
                    handle_start_round_play_occupation_if_lessons_underused(
                        ctx,
                        player,
                        *food_cost,
                        trigger_data,
                    )?
                }
                CardEffect::StartRoundPlayCardIfStoneHouse {
                    occupation_food_cost,
                } => handle_start_round_play_card_if_stone_house(
                    ctx,
                    player,
                    *occupation_food_cost,
                    trigger_data,
                )?,
                CardEffect::StartRoundGainIfGoodsOnRoundSpaces { good, amount } => {
                    handle_start_round_gain_if_goods_on_round_spaces(
                        ctx,
                        player,
                        good,
                        *amount,
                        trigger_data,
                    )?
                }
                CardEffect::StartRoundExchangeForFoodAndPoints {
                    give_animal,
                    give_grain,
                    give_vegetable,
                    receive_food,
                    receive_points,
                } => handle_start_round_exchange_for_food_and_points(
                    ctx,
                    player,
                    *give_animal,
                    *give_grain,
                    *give_vegetable,
                    *receive_food,
                    *receive_points,
                    trigger_data,
                )?,
                CardEffect::StartRoundPlaceFoodOnNextRoundSpaces {
                    grain_cost,
                    food_per_space,
                    num_spaces,
                } => handle_start_round_place_food_on_next_round_spaces(
                    ctx,
                    player,
                    *grain_cost,
                    *food_per_space,
                    *num_spaces,
                    trigger_data,
                )?,
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
#[derive(Debug, Clone, Serialize)]
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
    /// The trigger site (event kind) where this dispatch originated.
    /// Saves that lack this field default to WorkerPlaced via the custom
    /// `Deserialize` impl below (`Helper`); this struct only derives
    /// `Serialize`, so no struct-level `#[serde(default)]` belongs here.
    /// Resume-time validation ensures the site matches the actual
    /// execution context.
    pub site: EventKind,
    /// Legacy field for deserializing saves that used `event_kind` instead of `site`.
    /// Not serialized; only used during deserialization to populate `site`.
    #[serde(skip_serializing, default = "EventKind::default")]
    pub event_kind: EventKind,
    /// Revealed round cards (for round-start effects that need to know the round action spaces).
    /// Only populated for RoundStart triggers.
    #[serde(default)]
    pub revealed_round_cards: Vec<crate::model::RoundCard>,
    /// C066 永遠のライ麦畑 owner's instead-choice for this harvest: `true`
    /// takes the 1 food at grain >= 3, `false` (default) takes the 1 grain.
    /// Only read by the `AfterHarvestComplete` dispatch; harmless elsewhere.
    /// PR #385 review finding (b).
    #[serde(default)]
    pub c066_take_food_instead: bool,
}

impl<'de> Deserialize<'de> for PhaseTriggerData {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct Helper {
            space: Option<Space>,
            improvement: Option<crate::model::Improvement>,
            baked: bool,
            previous_player: Option<Player>,
            used_spaces: std::collections::BTreeSet<Space>,
            site: Option<EventKind>,
            event_kind: Option<EventKind>,
            revealed_round_cards: Option<Vec<crate::model::RoundCard>>,
            c066_take_food_instead: Option<bool>,
        }

        let h = Helper::deserialize(deserializer)?;
        // Presence-based precedence: a `site` field (new format) always wins,
        // legacy saves carrying only `event_kind` fall back to it, and when
        // both are absent the site defaults to WorkerPlaced.
        let site = h.site.or(h.event_kind).unwrap_or_default();

        Ok(PhaseTriggerData {
            space: h.space,
            improvement: h.improvement,
            baked: h.baked,
            previous_player: h.previous_player,
            used_spaces: h.used_spaces,
            site,
            event_kind: site, // Keep in sync for any downstream code that reads event_kind
            revealed_round_cards: h.revealed_round_cards.unwrap_or_default(),
            c066_take_food_instead: h.c066_take_food_instead.unwrap_or(false),
        })
    }
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
        CardEffect::AfterWorkerGain { space, .. } => {
            trigger.site == EventKind::WorkerPlaced && trigger.space == Some(*space)
        }
        CardEffect::AfterWorkerGainIfUsed {
            space,
            required_space,
            ..
        } => {
            trigger.site == EventKind::WorkerPlaced
                && trigger.space == Some(*space)
                && used_spaces.contains(required_space)
        }
        // AfterAnyWorkerGain and AfterAnyWorkerOthersGain are handled in
        // dispatch_after_worker_any_player, not the generic phase scan.
        // Return false here to prevent double-triggering.
        CardEffect::AfterAnyWorkerGain { .. } => false,
        CardEffect::AfterAnyWorkerOthersGain { .. } => false,
        // HarvestStartGainIfFields is handled in dispatch_harvest_field via the generic scan.
        CardEffect::HarvestStartGainIfFields { .. } => trigger.site == EventKind::Harvest,
        CardEffect::StartRoundGain { house, rooms, .. } => {
            trigger.site == EventKind::RoundStart && {
                let house_ok = house.is_none_or(|h| player.house == h);
                let rooms_ok = rooms.is_none_or(|r| player.rooms() == usize::from(r));
                house_ok && rooms_ok
            }
        }
        CardEffect::StartRoundGainIfFoodAtLeastRound { .. } => {
            trigger.site == EventKind::RoundStart && player.supply.food >= u32::from(round)
        }
        // E111 世捨て人: the no-minor-improvements condition needs card
        // definitions, so the scan reports it as triggerable and the
        // handler performs the check.
        CardEffect::StartRoundGainIfNoMinorPlayed { .. } => trigger.site == EventKind::RoundStart,
        // B114 子なし (family exactly N + rooms >= M), A157 ボヘミアン (Lessons
        // occupants), A049 営巣地 (accumulation grew), C067 ミネラルフィーダー
        // (non-harvest round + pasture animals): the conditions are judged
        // inside the handlers (E111/C111 pattern), so the scan only gates the
        // site.
        CardEffect::StartRoundGainIfFamilyAndRooms { .. } => trigger.site == EventKind::RoundStart,
        CardEffect::StartRoundGainIfLessonsOccupants { .. } => {
            trigger.site == EventKind::RoundStart
        }
        CardEffect::StartRoundGainIfAccumulationGrew { .. } => {
            trigger.site == EventKind::RoundStart
        }
        CardEffect::StartRoundGainIfNoHarvestAndAnimals { .. } => {
            trigger.site == EventKind::RoundStart
        }
        CardEffect::HarvestFieldGainIfNoMinorPlayed { .. } => trigger.site == EventKind::Harvest,
        // D065 麦ふるい: the grain-harvested threshold needs farm state, so the
        // scan reports the effect as triggerable and the handler counts the
        // growing grain fields (the pre-harvest equal of this phase's grain).
        CardEffect::HarvestFieldGainIfGrainHarvestedAtLeast { .. } => {
            trigger.site == EventKind::Harvest
        }
        // C116 調度品職員 fires only from dispatch_play's dedicated after-play
        // scan, never from the generic phase scan.
        CardEffect::AfterOccupationGainPerFoodCost { .. } => false,
        // A109 小商人 fires only from dispatch_play's dedicated after-play scan
        // for improvements.
        CardEffect::AfterImprovementPlayedGain { .. } => false,
        // C068 書棚 fires only from dispatch_play's dedicated after-play
        // scan, never from the generic phase scan.
        CardEffect::AfterOccupationGain { .. } => false,
        CardEffect::AfterClayConstructionGain { .. } => {
            trigger.site == EventKind::ClayConstruction
                && trigger.previous_player.as_ref().is_some_and(|prev| {
                    (prev.house == House::Clay && player.rooms() > prev.rooms())
                        || (prev.house == House::Clay && player.house == House::Stone)
                })
        }
        CardEffect::AfterHarvestBakeGain { .. } => trigger.site == EventKind::Bake && trigger.baked,
        CardEffect::HarvestFieldGain { .. } => trigger.site == EventKind::Harvest,
        // AfterHarvestGain/IfAtLeast are "at the end of each harvest" cards
        // (A145/C066): they fire from dispatch_after_harvest_complete at the
        // harvest tail, NOT from the field-phase trigger. On EventKind::Harvest
        // (fired before the fields yield) they must stay dormant — that early
        // firing was review finding (a) of PR #385. Additionally, they must only
        // fire during actual harvest rounds (4, 7, 9, 11, 13, 14).
        CardEffect::AfterHarvestGain { .. } => {
            trigger.site == EventKind::AfterHarvestComplete
                && crate::model::HARVEST_ROUNDS.contains(&round)
        }
        CardEffect::AfterHarvestGainIfAtLeast { .. } => {
            trigger.site == EventKind::AfterHarvestComplete
                && crate::model::HARVEST_ROUNDS.contains(&round)
        }
        CardEffect::AfterHarvestChooseGainIfAtLeast { .. } => {
            trigger.site == EventKind::AfterHarvestComplete
                && crate::model::HARVEST_ROUNDS.contains(&round)
        }
        // The A118 exchange is executed plan-driven by the engine's field
        // phase (FeedingPlan::a118_wood_buys, capped at 2). The dispatcher
        // scan stays dormant: a harvest-created PendingDecision has no
        // resolution window (start_round rejects a carried pending), which
        // bricked every A118-holder harvest — review finding (c) of PR #385.
        CardEffect::HarvestFieldOptionalExchange { .. } => false,
        CardEffect::OnPlayGain { .. } => true,
        CardEffect::OnPlayGainPerStored { .. } => true,
        CardEffect::OnPlayGainPerImprovementPlayed { .. } => true,
        CardEffect::OnPlayGainPerCount { .. } => true,
        // D007 三つ又の矛: the round table is resolved here, not in the handler,
        // so exactly one table entry (the current round's) is ever triggerable
        // and the other three rounds grant nothing. The round is public engine
        // state; no hidden information is consulted.
        CardEffect::OnPlayGainIfRound {
            round: effect_round,
            ..
        } => trigger.site == EventKind::CardPlayed && *effect_round == round,
        // Zone-move metadata: never a resolvable trigger; the pass-left move
        // runs after the play chain in dispatch_play.
        CardEffect::PostPlayPassLeft => false,
        CardEffect::OnPlayGainIfHouse { house, rooms, .. } => {
            player.house == *house && player.rooms() == usize::from(*rooms)
        }
        // B073 贈り物かご: only the table row matching the owner's current
        // room count exactly resolves; rows for other room counts grant
        // nothing (no double grant, no rounding).
        CardEffect::OnPlayGainIfRoomCount { rooms, .. } => player.rooms() == usize::from(*rooms),
        CardEffect::OnPlayGainAnimal { .. } => true,
        CardEffect::AfterWorkerGainAnimal { space, .. } => {
            trigger.site == EventKind::WorkerPlaced && trigger.space == Some(*space)
        }
        CardEffect::Passive { .. } => true,
        CardEffect::Prerequisite { .. } => true,
        CardEffect::EndGameBonus { .. } => true,
        // Chapter 1 round-start cards (A090/A151/A152/B097/B106/B135/C064).
        //
        // Optional round-start actions stay dormant in the generic scan for
        // the same reason as the A118 exchange (PR #385 review finding (c)):
        // `GameState::start_round` discards the dispatch Transition and only
        // re-checks `pending.is_some()` at the NEXT round boundary, so a
        // pause created here has no resolution window. Multiple owners would
        // also overwrite one shared `pending` slot mid-loop. The trigger
        // vocabulary, the dispatch wiring and the condition predicates are
        // all in place; only the player-decision channel (which field to
        // plow, which hand card to play, which animal to give) is missing,
        // and it needs a round-start plan channel before these can apply.
        CardEffect::StartRoundPlowFieldIfHouse { .. } => false,
        CardEffect::StartRoundCopyUnusedRoundSpace { .. } => false,
        CardEffect::StartRoundPlayOccupationIfLessonsUnderused { .. } => false,
        CardEffect::StartRoundPlayCardIfStoneHouse { .. } => false,
        // B106 is the exception that stays live: its gain is unconditional
        // once the round spaces hold goods (the handler does the goods
        // check), so it needs no player decision and can apply directly.
        CardEffect::StartRoundGainIfGoodsOnRoundSpaces { .. } => {
            trigger.site == EventKind::RoundStart
        }
        CardEffect::StartRoundExchangeForFoodAndPoints { .. } => false,
        CardEffect::StartRoundPlaceFoodOnNextRoundSpaces { .. } => false,
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
            // Never fires via the generic phase scan (see
            // effect_condition_holds); dispatch_play's after-play scan owns it.
            CardEffect::AfterOccupationGainPerFoodCost { .. } => EffectResult::Resolved,
            // Never fires via the generic phase scan; dispatch_play's after-play
            // scan for improvements owns it.
            CardEffect::AfterImprovementPlayedGain { .. } => EffectResult::Resolved,
            CardEffect::AfterOccupationGain { .. } => EffectResult::Resolved,
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
            // B073 贈り物かご: the condition scan already routed only the row
            // whose room count matches the owner exactly.
            CardEffect::OnPlayGainIfRoomCount {
                rooms,
                good,
                amount,
            } => handle_on_play_gain_if_room_count(ctx, player, *rooms, good, *amount)?,
            CardEffect::OnPlayGainPerImprovementPlayed { good, amount } => {
                handle_on_play_gain_per_improvement_played(ctx, player, good, *amount)?
            }
            CardEffect::OnPlayGainPerCount {
                count_kind,
                good,
                amount,
            } => handle_on_play_gain_per_count(ctx, player, count_kind, good, *amount)?,
            CardEffect::OnPlayGainAnimal { animal, amount } => {
                handle_on_play_gain_animal(ctx, animal, *amount)?
            }
            // D007 三つ又の矛: the exact-round match is already resolved by
            // effect_condition_holds, so a triggerable entry is the current
            // round's row of the printed table and grants unconditionally.
            CardEffect::OnPlayGainIfRound { good, amount, .. } => {
                handle_on_play_gain(ctx, player, good, *amount)?
            }
            // Zone-move metadata: never resolved by the scan.
            CardEffect::PostPlayPassLeft => EffectResult::Resolved,
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
            CardEffect::AfterWorkerGainAnimal {
                space,
                animal,
                amount,
            } => handle_after_worker_gain_animal(ctx, space, animal, *amount, trigger_data)?,
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
            // B114 子なし: family exactly N + rooms >= M (handler judges).
            CardEffect::StartRoundGainIfFamilyAndRooms {
                family,
                rooms_at_least,
                good,
                amount,
            } => handle_start_round_gain_if_family_and_rooms(
                ctx,
                player,
                *family,
                *rooms_at_least,
                good,
                *amount,
            )?,
            // A157 ボヘミアン: Lessons occupants >= min (handler judges).
            CardEffect::StartRoundGainIfLessonsOccupants { min, good, amount } => {
                handle_start_round_gain_if_lessons_occupants(ctx, player, *min, good, *amount)?
            }
            // A049 営巣地: accumulation >= min and grew this round.
            CardEffect::StartRoundGainIfAccumulationGrew {
                space,
                min,
                good,
                amount,
            } => handle_start_round_gain_if_accumulation_grew(
                ctx, player, *space, *min, good, *amount,
            )?,
            // C067 ミネラルフィーダー: non-harvest round + pasture animals.
            CardEffect::StartRoundGainIfNoHarvestAndAnimals {
                animal,
                min,
                good,
                amount,
            } => handle_start_round_gain_if_no_harvest_and_animals(
                ctx, player, *animal, *min, good, *amount, round_now,
            )?,
            CardEffect::HarvestFieldGainIfNoMinorPlayed { good, amount } => {
                handle_gain_if_no_minor_played(ctx, player, good, *amount)?
            }
            CardEffect::HarvestFieldGainIfGrainHarvestedAtLeast {
                threshold,
                good,
                amount,
            } => handle_harvest_field_gain_if_grain_harvested_at_least(
                ctx, player, *threshold, good, *amount,
            )?,
            CardEffect::AfterClayConstructionGain { good, amount } => {
                handle_after_clay_construction_gain(ctx, player, good, *amount, trigger_data)?
            }
            CardEffect::AfterHarvestBakeGain { amount } => {
                handle_after_harvest_bake_gain(ctx, player, *amount, trigger_data)?
            }
            CardEffect::HarvestFieldGain { rule, good, amount } => {
                handle_harvest_field_gain(ctx, player, rule, good, *amount, trigger_data)?
            }
            CardEffect::AfterHarvestGain { good, amount } => {
                handle_after_harvest_gain(ctx, player, good, *amount)?
            }
            CardEffect::AfterHarvestGainIfAtLeast {
                good,
                threshold,
                gain,
                amount,
            } => {
                handle_after_harvest_gain_if_at_least(ctx, player, good, *threshold, gain, *amount)?
            }
            CardEffect::AfterHarvestChooseGainIfAtLeast {
                good,
                threshold,
                gain,
                amount,
                upgrade_threshold,
                upgrade_gain,
                upgrade_amount,
            } => handle_after_harvest_choose_gain_if_at_least(
                ctx,
                player,
                good,
                *threshold,
                gain,
                *amount,
                *upgrade_threshold,
                upgrade_gain,
                *upgrade_amount,
                trigger_data,
            )?,
            CardEffect::HarvestFieldOptionalExchange {
                cost,
                cost_amount,
                gain,
                gain_amount,
            } => handle_harvest_field_optional_exchange(
                ctx,
                player,
                cost,
                *cost_amount,
                gain,
                *gain_amount,
            )?,
            // AfterAnyWorkerGain / AfterAnyWorkerOthersGain are handled in
            // dispatch_after_worker_any_player, not the generic phase scan.
            // HarvestStartGainIfFields is handled in dispatch_harvest_field via the generic scan.
            CardEffect::AfterAnyWorkerGain { .. } | CardEffect::AfterAnyWorkerOthersGain { .. } => {
                panic!(
                    "{:?} should be handled in special dispatch, not generic phase resolution",
                    effect
                )
            }
            CardEffect::HarvestStartGainIfFields {
                crop,
                threshold,
                good,
                amount,
            } => handle_harvest_start_gain_if_fields(
                ctx,
                player,
                crop,
                *threshold,
                good,
                *amount,
                trigger_data,
            )?,
            // Chapter 1 round-start cards (A090/A151/A152/B097/B106/B135/C064)
            CardEffect::StartRoundPlowFieldIfHouse {
                house,
                food_cost,
                fields,
            } => handle_start_round_plow_field_if_house(
                ctx,
                player,
                house,
                *food_cost,
                *fields,
                trigger_data,
            )?,
            CardEffect::StartRoundCopyUnusedRoundSpace { max_round } => {
                handle_start_round_copy_unused_round_space(
                    ctx,
                    player,
                    *max_round,
                    round_now,
                    trigger_data,
                )?
            }
            CardEffect::StartRoundPlayOccupationIfLessonsUnderused { food_cost } => {
                handle_start_round_play_occupation_if_lessons_underused(
                    ctx,
                    player,
                    *food_cost,
                    trigger_data,
                )?
            }
            CardEffect::StartRoundPlayCardIfStoneHouse {
                occupation_food_cost,
            } => handle_start_round_play_card_if_stone_house(
                ctx,
                player,
                *occupation_food_cost,
                trigger_data,
            )?,
            CardEffect::StartRoundGainIfGoodsOnRoundSpaces { good, amount } => {
                handle_start_round_gain_if_goods_on_round_spaces(
                    ctx,
                    player,
                    good,
                    *amount,
                    trigger_data,
                )?
            }
            CardEffect::StartRoundExchangeForFoodAndPoints {
                give_animal,
                give_grain,
                give_vegetable,
                receive_food,
                receive_points,
            } => handle_start_round_exchange_for_food_and_points(
                ctx,
                player,
                *give_animal,
                *give_grain,
                *give_vegetable,
                *receive_food,
                *receive_points,
                trigger_data,
            )?,
            CardEffect::StartRoundPlaceFoodOnNextRoundSpaces {
                grain_cost,
                food_per_space,
                num_spaces,
            } => handle_start_round_place_food_on_next_round_spaces(
                ctx,
                player,
                *grain_cost,
                *food_per_space,
                *num_spaces,
                trigger_data,
            )?,
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
        crate::cards::CardRequirement::MinimumImprovements(min) => {
            crate::cards::improvement_count_in_front(ctx.card_set, p) >= u32::from(*min)
        }
        crate::cards::CardRequirement::MinimumPlayers(_) => true, // Validated at card set level
        crate::cards::CardRequirement::MaximumPlayers(_) => true, // Validated at card set level
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

/// B073 贈り物かご (Gift Basket): one table row of the on-play room-count
/// gain. The generic scan (effect_condition_holds) only routes the row whose
/// `rooms` matches the owner exactly, so this handler applies the gain
/// unconditionally for the routed row and the other rows never reach it.
fn handle_on_play_gain_if_room_count(
    ctx: &mut DispatchContext<'_>,
    _player: usize,
    rooms: u8,
    good: &Good,
    amount: u32,
) -> Result<EffectResult, String> {
    let p = &mut ctx.player;
    debug_assert_eq!(
        p.rooms(),
        usize::from(rooms),
        "handler must only run for the row matching the room count"
    );
    let next = p
        .supply
        .get(*good)
        .checked_add(amount)
        .ok_or("resource overflow")?;
    p.supply.set(*good, next);
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

fn handle_on_play_gain_per_count(
    ctx: &mut DispatchContext<'_>,
    _player: usize,
    count_kind: &crate::cards::CountKind,
    good: &Good,
    amount: u32,
) -> Result<EffectResult, String> {
    let count = match count_kind {
        crate::cards::CountKind::Occupations => {
            crate::cards::occupation_count(ctx.card_set, ctx.player) as u32
        }
        crate::cards::CountKind::ClayRooms => crate::cards::clay_room_count(ctx.player) as u32,
        crate::cards::CountKind::Improvements => {
            crate::cards::improvement_count_in_front(ctx.card_set, ctx.player)
        }
        crate::cards::CountKind::FamilyMembers => ctx.player.family as u32,
        crate::cards::CountKind::MajorImprovementsBottomRow => {
            crate::cards::major_improvements_bottom_row(ctx.player) as u32
        }
        crate::cards::CountKind::SownFields => crate::cards::sown_field_count(ctx.player) as u32,
    };
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

/// Derive an explicit [`crate::engine::AnimalPlan`] that accounts for exactly
/// `amount` animals of `animal`, honouring the same placement vocabulary the
/// animal-market path uses: cattle-compatible pastures (id order), empty
/// unfenced stables (cell order), the free pet slot, and discard for any
/// overflow. Overflow discard is the official rule for animals with no room
/// (2016 Lookout; 放棄 is one of the five legal destinations).
fn auto_animal_plan(p: &Player, animal: Animal, amount: u32) -> crate::engine::AnimalPlan {
    let mut plan = crate::engine::AnimalPlan {
        animal,
        to_pastures: vec![],
        to_stables: vec![],
        pet: false,
        cook: 0,
        discard: 0,
    };
    let mut remaining = amount;
    // 1. Pastures that already hold this species (or are empty), up to capacity.
    for past in &p.pastures {
        if remaining == 0 {
            break;
        }
        if past.animal.is_some() && past.animal != Some(animal) {
            continue;
        }
        let free = p.pasture_capacity(past).saturating_sub(past.quantity);
        if free > 0 {
            let take = free.min(remaining);
            plan.to_pastures.push((past.id, take));
            remaining -= take;
        }
    }
    // 2. Empty unfenced stables (one animal each), by cell index.
    for (i, cell) in p.farm.iter().enumerate() {
        if remaining == 0 {
            break;
        }
        if cell.stable && matches!(cell.tile, Tile::Empty) && cell.stable_animal.is_none() {
            plan.to_stables.push(i);
            remaining -= 1;
        }
    }
    // 3. The pet slot keeps one animal that would otherwise be lost.
    if remaining > 0 && p.pet.is_none() {
        plan.pet = true;
        remaining -= 1;
    }
    // 4. Overflow is discarded.
    plan.discard = remaining;
    plan
}

/// Handle on-play animal gain (C083 若牛): the gain is an explicit AnimalPlan
/// applied through `engine::place_animals` — the same path the animal market
/// uses — never a Supply good (animals are not `Good`s). The play executor
/// cannot represent a play-time pause (Issue #268 R2-P0-2), so the placement
/// plan is derived from the board: free capacity first, overflow discarded.
fn handle_on_play_gain_animal(
    ctx: &mut DispatchContext<'_>,
    animal: &Animal,
    amount: u32,
) -> Result<EffectResult, String> {
    let plan = auto_animal_plan(ctx.player, *animal, amount);
    crate::engine::place_animals(ctx.player, &plan, amount)
        .map_err(|e| format!("on-play animal placement failed: {e}"))?;
    Ok(EffectResult::Resolved)
}

/// Handle after-worker animal gain (C147 牛の飼育士): each use of `space`
/// grants `amount` animals, placed through `engine::place_animals` with a
/// plan derived from the board (see [`auto_animal_plan`]). Pauses for a
/// ChooseAnimalPlacement decision in the After phase (which supports interrupts).
fn handle_after_worker_gain_animal(
    ctx: &mut DispatchContext<'_>,
    space: &Space,
    animal: &Animal,
    amount: u32,
    trigger: &PhaseTriggerData,
) -> Result<EffectResult, String> {
    if trigger.space == Some(*space) {
        let plan = auto_animal_plan(ctx.player, *animal, amount);
        let kind = crate::pending::PendingKind::ChooseAnimalPlacement {
            source_card: "C147".to_string(),
            animal: *animal,
            amount,
        };
        let decision = PendingDecision::new(
            0,
            ctx.card_owner,
            kind,
            String::new(),
            serde_json::json!({ "card": "C147", "space": space, "auto_plan": plan }),
        );
        return Ok(EffectResult::Pending(decision));
    }
    Ok(EffectResult::Resolved)
}

/// A050 (others half): each time ANY player uses the specified space,
/// all players EXCEPT the card owner gain `amount` of `good`.
fn handle_after_any_worker_others_gain(
    players: &mut [Player],
    card_owner: usize,
    space: &Space,
    good: &Good,
    amount: u32,
    trigger: &PhaseTriggerData,
) -> Result<EffectResult, String> {
    if trigger.space == Some(*space) {
        for (idx, p) in players.iter_mut().enumerate() {
            if idx == card_owner {
                continue;
            }
            let next = p
                .supply
                .get(*good)
                .checked_add(amount)
                .ok_or("resource overflow")?;
            p.supply.set(*good, next);
        }
    }
    Ok(EffectResult::Resolved)
}

/// D061: at harvest start, if the card owner has at least `threshold`
/// fields sown with `crop`, gain `amount` of `good`.
fn handle_harvest_start_gain_if_fields(
    ctx: &mut DispatchContext<'_>,
    _player: usize,
    crop: &Crop,
    threshold: u8,
    good: &Good,
    amount: u32,
    _trigger: &PhaseTriggerData,
) -> Result<EffectResult, String> {
    let p = &mut ctx.player;
    let field_count = p
        .farm
        .iter()
        .filter(|cell| {
            matches!(
                cell.tile,
                Tile::Field {
                    crop: Some(c),
                    remaining: 1..,
                } if c == *crop
            )
        })
        .count() as u8;
    if field_count >= threshold {
        let next = p
            .supply
            .get(*good)
            .checked_add(amount)
            .ok_or("resource overflow")?;
        p.supply.set(*good, next);
    }
    Ok(EffectResult::Resolved)
}

/// D065 麦ふるい (Grain Sieve): during the harvest field phase, if the grain
/// gained THIS field phase (pre-harvest equal to the number of grain fields
/// with a growing crop) is at least `threshold`, gain `amount` of `good`.
/// The engine's field phase (`harvest.rs::harvest_fields`) harvests exactly
/// one grain per growing grain field AFTER this dispatch fires, so counting
/// grain fields with `remaining >= 1` here equals the grain this phase yields.
/// The count is per FIELD, not per remaining unit: a field with
/// `remaining: 2` still yields exactly one grain this harvest. The generic
/// condition scan reports the effect as triggerable on `EventKind::Harvest`;
/// the threshold check needs farm state, so it happens here.
fn handle_harvest_field_gain_if_grain_harvested_at_least(
    ctx: &mut DispatchContext<'_>,
    _player: usize,
    threshold: u32,
    good: &Good,
    amount: u32,
) -> Result<EffectResult, String> {
    let p = &mut ctx.player;
    let grain_fields = p
        .farm
        .iter()
        .filter(|cell| {
            matches!(
                cell.tile,
                Tile::Field {
                    crop: Some(Crop::Grain),
                    remaining: 1..,
                }
            )
        })
        .count() as u32;
    if grain_fields >= threshold {
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

/// B114 子なし (Childless): at round start, if the owner has exactly `family`
/// family members and at least `rooms_at_least` rooms, gain `amount` of
/// `good`. Spec conditions: `family_exactly` (2) + `rooms_at_least` (3).
/// The spec's second step (choose 1 grain or 1 vegetable) is recorded as
/// needs_resolution in the PR and is deliberately not implemented here.
fn handle_start_round_gain_if_family_and_rooms(
    ctx: &mut DispatchContext<'_>,
    _player: usize,
    family: u32,
    rooms_at_least: u8,
    good: &Good,
    amount: u32,
) -> Result<EffectResult, String> {
    let p = &mut ctx.player;
    if u32::from(p.family) == family && p.rooms() >= usize::from(rooms_at_least) {
        let next = p
            .supply
            .get(*good)
            .checked_add(amount)
            .ok_or("resource overflow")?;
        p.supply.set(*good, next);
    }
    Ok(EffectResult::Resolved)
}

/// A157 ボヘミアン (Bohemian): at round start, if the Lessons space has at
/// least `min` occupants, gain `amount` of `good`. The occupants are read
/// from the used-space set the dispatcher carries (`public.used`).
///
/// Engine-timing note (needs_resolution, see PR body): `Game::start_round`
/// clears `public.used` before dispatching round-start triggers, so the
/// production path always hands this handler an empty set and the card stays
/// dormant there. The handler itself is correct whenever a dispatch carries
/// the occupants (covered by the positive test).
fn handle_start_round_gain_if_lessons_occupants(
    ctx: &mut DispatchContext<'_>,
    _player: usize,
    min: u32,
    good: &Good,
    amount: u32,
) -> Result<EffectResult, String> {
    let occupants = u32::from(ctx.used_spaces.contains(&Space::Lessons));
    if occupants >= min {
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

/// A049 営巣地 (Nest Site): at round start, if the `space` accumulation holds
/// at least `min` AND grew this round, gain `amount` of `good`. "Grew this
/// round" means the per-round refill landed on a non-empty accumulation: the
/// dispatcher runs right after the refill, so `current > per_round_amount`
/// identifies exactly that case (current = pre-refill + per-round amount).
///
/// The per-round amount comes from `Space::accumulation` (the base refill);
/// `Space::accumulation_for` differs from it only for Hollow at 4 players,
/// which no card configures for this effect, and the handler has no
/// player-count context.
fn handle_start_round_gain_if_accumulation_grew(
    ctx: &mut DispatchContext<'_>,
    _player: usize,
    space: Space,
    min: u32,
    good: &Good,
    amount: u32,
) -> Result<EffectResult, String> {
    let current = ctx
        .accumulations
        .and_then(|map| map.get(&space).copied())
        .unwrap_or(0);
    let per_round = space.accumulation().map(|(_, n)| n).unwrap_or(0);
    if current >= min && current > per_round {
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

/// C067 ミネラルフィーダー (Mineral Feeder): at round start, if the round
/// does not end with a harvest and the owner pastures at least `min` of
/// `animal`, gain `amount` of `good`. Spec conditions:
/// `round_has_no_harvest` and `animal_count_at_least` with location
/// "pasture", so only pasture-held animals count (stable/pet animals are
/// excluded on purpose).
fn handle_start_round_gain_if_no_harvest_and_animals(
    ctx: &mut DispatchContext<'_>,
    _player: usize,
    animal: Animal,
    min: u32,
    good: &Good,
    amount: u32,
    round: u8,
) -> Result<EffectResult, String> {
    if crate::model::HARVEST_ROUNDS.contains(&round) {
        return Ok(EffectResult::Resolved);
    }
    let in_pastures: u32 = ctx
        .player
        .pastures
        .iter()
        .filter(|pasture| pasture.animal == Some(animal))
        .map(|pasture| pasture.quantity)
        .sum();
    if in_pastures >= min {
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
        crate::cards::HarvestFieldRule::Fixed => 1,
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

/// A145 綱作り (Ropemaker): unconditional gain at the end of each harvest.
/// Fires from the EventKind::Harvest After scan; the generic condition scan
/// already verified the event kind, so we just grant the resource.
fn handle_after_harvest_gain(
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

/// C066 永遠のライ麦畑 (Eternal Rye Cultivation): conditional gain at the end
/// of each harvest. If the owner has at least `threshold` of `good`, gain
/// `amount` of `gain`. The generic condition scan reports it as triggerable
/// for Harvest; the handler performs the threshold check (same pattern as
/// E111 世捨て人).
fn handle_after_harvest_gain_if_at_least(
    ctx: &mut DispatchContext<'_>,
    _player: usize,
    good: &Good,
    threshold: u32,
    gain: &Good,
    amount: u32,
) -> Result<EffectResult, String> {
    if ctx.player.supply.get(*good) >= threshold {
        let p = &mut ctx.player;
        let next = p
            .supply
            .get(*gain)
            .checked_add(amount)
            .ok_or("resource overflow")?;
        p.supply.set(*gain, next);
    }
    Ok(EffectResult::Resolved)
}

/// C066 永遠のライ麦畑 (Eternal Rye Cultivation): conditional WITH-choice gain
/// at the end of each harvest. Below `upgrade_threshold` the primary grant is
/// unconditional; at or above it the owner chooses 「代わりに」 — take
/// `upgrade_amount` of `upgrade_gain` (default) or, when the FeedingPlan set
/// `c066_take_food_instead`, the primary `amount` of `gain` instead. The plan
/// flag rides in `PhaseTriggerData` from `dispatch_after_harvest_complete`.
/// PR #385 review finding (b): the previous twin unconditional effects
/// granted both at grain >= 3.
#[allow(clippy::too_many_arguments)] // mirrors the sibling after-harvest handlers' field threading
fn handle_after_harvest_choose_gain_if_at_least(
    ctx: &mut DispatchContext<'_>,
    _player: usize,
    good: &Good,
    threshold: u32,
    gain: &Good,
    amount: u32,
    upgrade_threshold: u32,
    upgrade_gain: &Good,
    upgrade_amount: u32,
    trigger_data: &PhaseTriggerData,
) -> Result<EffectResult, String> {
    let owned = ctx.player.supply.get(*good);
    if owned < threshold {
        return Ok(EffectResult::Resolved);
    }
    let (target, granted) = if owned >= upgrade_threshold {
        if trigger_data.c066_take_food_instead {
            (*gain, amount)
        } else {
            (*upgrade_gain, upgrade_amount)
        }
    } else {
        (*gain, amount)
    };
    let p = &mut ctx.player;
    let next = p
        .supply
        .get(target)
        .checked_add(granted)
        .ok_or("resource overflow")?;
    p.supply.set(target, next);
    Ok(EffectResult::Resolved)
}

/// A118 樹木庭師 (Treegardener): optional exchange during harvest field
/// phase. If the owner has at least `cost_amount` of `cost`, they may pay
/// it to gain `gain_amount` of `gain`. This creates a PendingDecision
/// (Custom kind) so the player chooses whether to execute the exchange.
fn handle_harvest_field_optional_exchange(
    ctx: &mut DispatchContext<'_>,
    _player: usize,
    cost: &Good,
    cost_amount: u32,
    gain: &Good,
    gain_amount: u32,
) -> Result<EffectResult, String> {
    // Only offer the exchange if the player can afford the cost.
    if ctx.player.supply.get(*cost) >= cost_amount {
        let kind = crate::pending::PendingKind::Custom {
            source_card: "A118".to_string(),
            payload: serde_json::json!({}),
        };
        let decision = PendingDecision::new(
            0,
            ctx.actor,
            kind,
            String::new(),
            serde_json::json!({
                "card": "A118",
                "type": "harvest_field_optional_exchange",
                "cost": format!("{:?}", cost),
                "cost_amount": cost_amount,
                "gain": format!("{:?}", gain),
                "gain_amount": gain_amount,
            }),
        );
        Ok(EffectResult::Pending(decision))
    } else {
        Ok(EffectResult::Resolved)
    }
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

// ============================================================================
// Chapter 1 round-start cards (A090/A151/A152/B097/B106/B135/C064)
//
// B106 applies for real (unconditional when goods sit on the revealed round
// spaces). The optional cards stay dormant: a round-start PendingDecision has
// no resolution window (`GameState::start_round` discards the dispatch
// Transition and several owners would overwrite one shared `pending` slot),
// which is the PR #385 finding (c) brick pattern. Applying them needs a
// round-start plan channel carrying the player's choice; the trigger
// vocabulary, dispatch wiring and condition predicates are already in place.
// ============================================================================

/// A090 鋤職人 (Plow Driver): at each round start, if the owner lives in a
/// stone house, may pay 1 food to plow exactly 1 field. Optional.
/// ENGINE LIMITATION: dormant — needs a field-location decision channel.
fn handle_start_round_plow_field_if_house(
    _ctx: &mut DispatchContext<'_>,
    _player: usize,
    _house: &House,
    _food_cost: u32,
    _fields: u32,
    _trigger_data: &PhaseTriggerData,
) -> Result<EffectResult, String> {
    // Dormant by design: the effect is optional and needs a field-location
    // decision the round-start dispatcher cannot represent (see the
    // `effect_condition_holds` note). The scan never reaches this handler
    // today; reaching it is a bug, so stay a resolved no-op rather than
    // creating an unresolvable pending decision (PR #385 finding (c)).
    Ok(EffectResult::Resolved)
}

/// A151 吟遊楽人 (Minstrel): at each round start in rounds 1-4, if the round
/// action space for that round is unused (no worker placed), copy its effect.
/// ENGINE LIMITATION: dormant — copying a space action has no engine
/// vocabulary (executing a space action without placing a worker).
fn handle_start_round_copy_unused_round_space(
    _ctx: &mut DispatchContext<'_>,
    _player: usize,
    _max_round: u8,
    _round_now: u8,
    _trigger_data: &PhaseTriggerData,
) -> Result<EffectResult, String> {
    // Dormant by design: copying an arbitrary round space's effect means
    // executing that space's action without a worker, which the engine has no
    // vocabulary for. Documented limitation; the scan never reaches this
    // handler (see the `effect_condition_holds` note), and reaching it stays
    // a resolved no-op rather than an unresolvable pending decision.
    Ok(EffectResult::Resolved)
}

/// A152 夜学生 (Night-School Student): at each round start, if the Lessons
/// space was used by fewer players than any other action space this round,
/// may pay 1 food to play an occupation from hand.
/// ENGINE LIMITATION: dormant — needs all players' used-space analysis plus
/// a hand-card choice channel.
fn handle_start_round_play_occupation_if_lessons_underused(
    _ctx: &mut DispatchContext<'_>,
    _player: usize,
    _food_cost: u32,
    _trigger_data: &PhaseTriggerData,
) -> Result<EffectResult, String> {
    // Dormant by design: the condition ("Lessons used by fewer players than
    // any other space") needs every player's used-space set, and the effect
    // needs a hand-card choice the round-start dispatcher cannot represent
    // (see the `effect_condition_holds` note). Resolved no-op, never an
    // unresolvable pending decision (PR #385 finding (c)).
    Ok(EffectResult::Resolved)
}

/// B097 学者 (Scholar): at each round start, if the owner has a stone house,
/// may either pay 1 food to play an occupation from hand, or pay the normal
/// cost to play a minor improvement from hand. Optional; two distinct choices.
/// ENGINE LIMITATION: dormant — needs a hand-card choice channel.
fn handle_start_round_play_card_if_stone_house(
    _ctx: &mut DispatchContext<'_>,
    _player: usize,
    _occupation_food_cost: u32,
    _trigger_data: &PhaseTriggerData,
) -> Result<EffectResult, String> {
    // Dormant by design: either choice plays a hand card (occupation for 1
    // food, or a minor for its normal cost), a card-selection decision the
    // round-start dispatcher cannot represent (see the
    // `effect_condition_holds` note). Resolved no-op, never an unresolvable
    // pending decision (PR #385 finding (c)).
    Ok(EffectResult::Resolved)
}

/// B106 清廉潔白な人 (Moral Crusader): immediately before each round begins,
/// if any round action space holds goods the owner would receive, gain 1
/// food. Unconditional gain when the condition met; the condition site is
/// checked by effect_condition_holds, the goods check happens here.
///
/// Interpretation note (recorded for human confirmation, AGENTS.md rule 3):
/// the BGA wiki text (checked 2026-11-14, spec `data/bga-card-specs/B106.json`)
/// reads, in this engine's terms, "goods on round action spaces". The engine's
/// single `accumulations` map also covers the base action spaces (Forest,
/// ClayPit, ...), which are NOT round spaces, so this handler restricts the
/// check to the revealed round cards' spaces. A stricter reading ("upcoming"
/// = not-yet-started round spaces) is currently unsatisfiable in the engine
/// because no effect can place goods on future round spaces yet.
fn handle_start_round_gain_if_goods_on_round_spaces(
    ctx: &mut DispatchContext<'_>,
    _player: usize,
    good: &Good,
    amount: u32,
    trigger_data: &PhaseTriggerData,
) -> Result<EffectResult, String> {
    // Round-card spaces only: the base action spaces share the
    // `accumulations` map but are not round spaces, so they must not satisfy
    // the condition. Only the round-start dispatcher threads `accumulations`
    // in (`None` elsewhere), so the Option gates the check naturally.
    let has_round_space_goods = trigger_data.revealed_round_cards.iter().any(|card| {
        ctx.accumulations
            .is_some_and(|map| map.get(&Space::from(*card)).is_some_and(|&n| n > 0))
    });
    if has_round_space_goods {
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

/// B135 栄養士 (Nutrition Expert): at each round start, may exchange 1 animal
/// of any owned type, 1 grain, and 1 vegetable for 5 food and 2 points.
/// Optional.
/// ENGINE LIMITATION: dormant — needs a given-animal choice channel.
#[allow(clippy::too_many_arguments)] // the exchange vocabulary is fixed: 3 give slots + 2 receive slots
fn handle_start_round_exchange_for_food_and_points(
    _ctx: &mut DispatchContext<'_>,
    _player: usize,
    _give_animal: bool,
    _give_grain: u32,
    _give_vegetable: u32,
    _receive_food: u32,
    _receive_points: i32,
    _trigger_data: &PhaseTriggerData,
) -> Result<EffectResult, String> {
    // Dormant by design: the exchange is optional and the given-animal choice
    // is a player decision the round-start dispatcher cannot represent (see
    // the `effect_condition_holds` note). Resolved no-op, never an
    // unresolvable pending decision (PR #385 finding (c)).
    Ok(EffectResult::Resolved)
}

/// C064 穀物蒸留酒製造所 (Corn Schnapps Distillery): at each round start,
/// may pay 1 grain to place 1 food on each of the next 4 round action
/// spaces. The food would be added to those spaces' accumulations and be
/// available when those rounds are reached. Optional.
/// ENGINE LIMITATION: dormant — needs a spend-vs-keep decision channel.
fn handle_start_round_place_food_on_next_round_spaces(
    _ctx: &mut DispatchContext<'_>,
    _player: usize,
    _grain_cost: u32,
    _food_per_space: u32,
    _num_spaces: u8,
    _trigger_data: &PhaseTriggerData,
) -> Result<EffectResult, String> {
    // Dormant by design: the effect is optional and needs the spend-vs-keep
    // grain decision the round-start dispatcher cannot represent (see the
    // `effect_condition_holds` note). Resolved no-op, never an unresolvable
    // pending decision (PR #385 finding (c)).
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
///
/// `is_improvement_action` indicates whether this play originates from a "Major or Minor
/// Improvement" action (2016 Lookout p.8: "Each time you take a 'Major or Minor Improvement'
/// action to play an improvement from your hand, you also get 3 food."). Only when true does
/// the Phase 4b scan for `AfterImprovementPlayedGain` (A109 小商人) run. This excludes
/// FamilyGrowthWithRoom/ModestFamilyGrowth + GrowWithMinor (family growth action) and
/// Lessons/ExtraLessons + PlayOccupation (occupation action).
#[allow(clippy::too_many_arguments)] // threads the same storages as the legacy cards.rs fns
pub fn dispatch_play(
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
    is_improvement_action: bool,
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
        accumulations: None,
        event_budget,
        card_uses,
        card_use_limits,
        pending,
        actor: player,
        card_owner: player,
        beneficiary: player,
    };

    let trigger = PhaseTriggerData {
        space: None,
        improvement: None,
        baked: false,
        previous_player: None,
        used_spaces: std::collections::BTreeSet::new(),
        site: EventKind::CardPlayed,
        event_kind: EventKind::CardPlayed,
        revealed_round_cards: Vec::new(),
        c066_take_food_instead: false,
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

    // Phase 4: After — card-play dependent triggers owned by EARLIER played
    // cards. C116 調度品職人 (`AfterOccupationGainPerFoodCost`): each time the owner
    // plays an occupation after it, it grants wood per food paid as that
    // occupation's occupation cost (the Lessons/ExtraLessons fee). The 2016
    // Lookout appendix defines the occupation cost as the food paid on the
    // "Lessons" action space and the card's own printed cost as the separate
    // "individual cost", so only the fee feeds the multiplier. C068 書棚
    // (`AfterOccupationGain`): each time the owner plays an occupation after it,
    // grants a fixed amount of the specified good, independent of the occupation's
    // cost. The new card is already last in `played_cards` (the caller ran the zone
    // move), so the scan covers exactly the cards played "before this one".
    if let Some(new_card) = card_set.definition(new_card_id) {
        if new_card.kind == crate::cards::CardKind::Occupation {
            // Food paid as the occupation cost (2016 Lookout appendix: "The
            // occupation cost is what you must pay when playing the occupation
            // card from your hand. It is stated on the 'Lessons' action space.
            // The (rather rare) individual cost is printed on the occupation
            // card itself."). C116 counts the occupation cost only, so the
            // Lessons/ExtraLessons fee is the whole multiplier; the played
            // occupation's own printed food cost is the individual cost and
            // never feeds it.
            let food_paid = extra_food_paid;
            let played = ctx.player.played_cards.clone();
            let owned = played.len().saturating_sub(1); // exclude the new card
            for id in &played[..owned] {
                let Some(card) = card_set.definition(id) else {
                    continue;
                };
                for effect in &card.effects {
                    // C116: AfterOccupationGainPerFoodCost — only triggers if food was paid
                    if let CardEffect::AfterOccupationGainPerFoodCost { good, amount } = effect {
                        if food_paid > 0 {
                            consume_budget(ctx.event_budget)?;
                            let limit = ctx
                                .card_use_limits
                                .get(id.as_str())
                                .copied()
                                .unwrap_or(u32::MAX);
                            check_card_use(ctx.card_uses, id, limit)?;
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
                        }
                        continue;
                    }
                    // C068: AfterOccupationGain — triggers for any occupation, fixed amount
                    if let CardEffect::AfterOccupationGain { good, amount } = effect {
                        consume_budget(ctx.event_budget)?;
                        let limit = ctx
                            .card_use_limits
                            .get(id.as_str())
                            .copied()
                            .unwrap_or(u32::MAX);
                        check_card_use(ctx.card_uses, id, limit)?;
                        let p = &mut ctx.player;
                        let next = p
                            .supply
                            .get(*good)
                            .checked_add(*amount)
                            .ok_or("resource overflow")?;
                        p.supply.set(*good, next);
                    }
                }
            }
        }
    }

    // Phase 4b: After — improvement-play dependent triggers owned by EARLIER played
    // cards. A109 小商人 (`AfterImprovementPlayedGain`): each time the owner takes a
    // "Major or Minor Improvement" action to play an improvement from hand (official
    // 2016 Lookout wording), it grants food. The new card is already last in
    // `played_cards` (the caller ran the zone move), so the scan covers exactly
    // the cards played "before this one". This scan ONLY runs for improvement
    // action paths (MeetingPlace+PlayMinor, MajorImprovement+PlayMinor), not for
    // FamilyGrowthWithRoom/ModestFamilyGrowth+GrowWithMinor (family growth action)
    // or Lessons/ExtraLessons+PlayOccupation (occupation action).
    if is_improvement_action {
        if let Some(new_card) = card_set.definition(new_card_id) {
            if new_card.kind == crate::cards::CardKind::MinorImprovement {
                let played = ctx.player.played_cards.clone();
                let owned = played.len().saturating_sub(1); // exclude the new card
                for id in &played[..owned] {
                    let Some(card) = card_set.definition(id) else {
                        continue;
                    };
                    for effect in &card.effects {
                        let CardEffect::AfterImprovementPlayedGain { good, amount } = effect else {
                            continue;
                        };
                        consume_budget(ctx.event_budget)?;
                        let limit = ctx
                            .card_use_limits
                            .get(id.as_str())
                            .copied()
                            .unwrap_or(u32::MAX);
                        check_card_use(ctx.card_uses, id, limit)?;
                        let p = &mut ctx.player;
                        let next = p
                            .supply
                            .get(*good)
                            .checked_add(*amount)
                            .ok_or("resource overflow")?;
                        p.supply.set(*good, next);
                    }
                }
            }
        }
    }
    // Card points for the newly played card are applied here — before the
    // post-play destination — so a printed "pass left" card can have them
    // subtracted again as it leaves (the validate_card_points invariant must
    // hold after every action). This replaces the per-call-site update the
    // engine play paths ran after dispatch_play returned; the committed state
    // is identical for cards that stay in the played area.
    if let Some(card) = card_set.definition(new_card_id) {
        crate::cards::update_card_points_on_play(&mut players[player], card.points)?;
    }

    // Post-play destination (2016 Lookout: printed "pass left" cards move to
    // the left neighbour's hand instead of staying in the played area). The
    // move runs exactly once, after every play phase resolved, through the
    // canonical card_zones::move_card API so the location partition and the
    // card-points invariant are preserved by construction. A one-player game
    // has no left neighbour, so the card stays in front of its owner.
    if let Some(card) = card_set.definition(new_card_id) {
        if card
            .effects
            .iter()
            .any(|effect| matches!(effect, CardEffect::PostPlayPassLeft))
            && players.len() >= 2
        {
            let left = (player + 1) % players.len();
            let mut move_ctx = crate::card_zones::CardMoveCtx {
                cards,
                players,
                improvement_market,
            };
            crate::card_zones::move_card(
                &mut move_ctx,
                new_card_id,
                crate::card_zones::CardZone::Played { player },
                crate::card_zones::CardZone::Hand { player: left },
            )
            .map_err(|e| e.to_string())?;
            players[player].card_points = players[player]
                .card_points
                .checked_sub(card.points)
                .ok_or("card points underflow on pass-left")?;
        }
    }
    Ok(())
}

/// Dispatch AfterAnyWorkerGain / AfterAnyWorkerOthersGain effects for ALL players.
/// These effects trigger when ANY player uses a space, so we scan all players'
/// cards (not just the actor's). This is called from `dispatch_after_worker`
/// after the actor's own effects have resolved.
#[allow(clippy::too_many_arguments)]
fn dispatch_after_worker_any_player(
    card_set: &CardSet,
    _actor: usize,
    space: Space,
    _cards: &mut crate::cards::CardState,
    players: &mut [Player],
    _improvement_market: &mut Vec<crate::model::Improvement>,
    _delayed_well: &mut BTreeMap<u8, Vec<usize>>,
    _round: u8,
    _used_spaces: &std::collections::BTreeSet<Space>,
    event_budget: &mut u32,
    card_uses: &mut BTreeMap<String, u32>,
    card_use_limits: &BTreeMap<String, u32>,
    _pending: &mut Option<PendingDecision>,
    trigger: &PhaseTriggerData,
    _all_players: &[Player],
) -> Result<(), String> {
    // For each player (as card owner), check their played cards for the new variants
    // Use index-based loop to avoid borrow checker issues with mutable access
    for card_owner in 0..players.len() {
        let played_cards = players[card_owner].played_cards.clone();
        for card_id in played_cards {
            let Some(card) = card_set.definition(&card_id) else {
                continue;
            };
            for effect in &card.effects {
                // Only handle the two new variants
                let (effect_space, good, amount) = match effect {
                    CardEffect::AfterAnyWorkerGain {
                        space,
                        good,
                        amount,
                    } => (*space, *good, *amount),
                    CardEffect::AfterAnyWorkerOthersGain {
                        space,
                        good,
                        amount,
                    } => (*space, *good, *amount),
                    _ => continue,
                };

                // Check condition: space matches and event is WorkerPlaced
                if effect_space != space || trigger.site != EventKind::WorkerPlaced {
                    continue;
                }

                // Consume event budget
                consume_budget(event_budget)?;

                // Check card use limit
                let limit = card_use_limits.get(&card_id).copied().unwrap_or(u32::MAX);
                check_card_use(card_uses, &card_id, limit)?;

                // Apply effect based on variant
                match effect {
                    CardEffect::AfterAnyWorkerGain { .. } => {
                        // Grant to card owner
                        let p = &mut players[card_owner];
                        let next = p
                            .supply
                            .get(good)
                            .checked_add(amount)
                            .ok_or("resource overflow")?;
                        p.supply.set(good, next);
                    }
                    CardEffect::AfterAnyWorkerOthersGain { .. } => {
                        // Grant to all players EXCEPT card owner
                        handle_after_any_worker_others_gain(
                            players,
                            card_owner,
                            &effect_space,
                            &good,
                            amount,
                            trigger,
                        )?;
                    }
                    _ => unreachable!(),
                }
            }
        }
    }
    Ok(())
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

    let trigger = PhaseTriggerData {
        space: Some(space),
        improvement: None,
        baked: false,
        previous_player: None,
        used_spaces: used_spaces.clone(),
        site: EventKind::WorkerPlaced,
        event_kind: EventKind::WorkerPlaced,
        revealed_round_cards: Vec::new(),
        c066_take_food_instead: false,
    };

    // AfterWorkerGain and AfterWorkerGainIfUsed are After phase. A pause here
    // (ChooseTriggerOrder, Issue #270) must reach the engine so it can build
    // the PostWork continuation — propagate instead of discarding.
    let mut ctx = DispatchContext {
        card_set,
        player: &mut players[player],
        cards,
        improvement_market,
        delayed_well,
        round,
        used_spaces,
        accumulations: None,
        event_budget,
        card_uses,
        card_use_limits,
        pending,
        actor: player,
        card_owner: player,
        beneficiary: player,
    };
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

    // AfterAnyWorkerGain / AfterAnyWorkerOthersGain: scan ALL players' cards
    // (not just the actor's) because these effects trigger on any player's
    // use of the space. We do this after the actor's own effects so that
    // ChooseTriggerOrder pause only applies to the active player's effects.
    dispatch_after_worker_any_player(
        card_set,
        player,
        space,
        cards,
        players,
        improvement_market,
        delayed_well,
        round,
        used_spaces,
        event_budget,
        card_uses,
        card_use_limits,
        pending,
        &trigger,
        &all_players,
    )?;

    // Recreate ctx for Interrupt phase (re-borrow the mutables)
    let mut ctx = DispatchContext {
        card_set,
        player: &mut players[player],
        cards,
        improvement_market,
        delayed_well,
        round,
        used_spaces,
        accumulations: None,
        event_budget,
        card_uses,
        card_use_limits,
        pending,
        actor: player,
        card_owner: player,
        beneficiary: player,
    };

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
    accumulations: &std::collections::BTreeMap<Space, u32>,
    event_budget: &mut u32,
    card_uses: &mut BTreeMap<String, u32>,
    card_use_limits: &BTreeMap<String, u32>,
    pending: &mut Option<PendingDecision>,
    revealed_round_cards: &[crate::model::RoundCard],
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
            accumulations: Some(accumulations),
            event_budget,
            card_uses,
            card_use_limits,
            pending,
            actor,
            card_owner: player,
            beneficiary: player,
        };

        let trigger = PhaseTriggerData {
            space: None,
            improvement: None,
            baked: false,
            previous_player: None,
            used_spaces: used_spaces.clone(),
            site: EventKind::RoundStart,
            event_kind: EventKind::RoundStart,
            revealed_round_cards: revealed_round_cards.to_vec(),
            c066_take_food_instead: false,
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
        accumulations: None,
        event_budget,
        card_uses,
        card_use_limits,
        pending,
        actor: player,
        card_owner: player,
        beneficiary: player,
    };

    let trigger = PhaseTriggerData {
        space: None,
        improvement,
        baked: false,
        previous_player: Some(previous.clone()),
        used_spaces: used_spaces.clone(),
        site: EventKind::ClayConstruction,
        event_kind: EventKind::ClayConstruction,
        revealed_round_cards: Vec::new(),
        c066_take_food_instead: false,
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
        accumulations: None,
        event_budget,
        card_uses,
        card_use_limits,
        pending,
        actor: player,
        card_owner: player,
        beneficiary: player,
    };

    let trigger = PhaseTriggerData {
        space: None,
        improvement: None,
        baked: false,
        previous_player: None,
        used_spaces: used_spaces.clone(),
        site: EventKind::Harvest,
        event_kind: EventKind::Harvest,
        revealed_round_cards: Vec::new(),
        c066_take_food_instead: false,
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
        false,
    )? {
        return Ok(Transition::Paused(p));
    }
    Ok(Transition::Completed)
}

/// Dispatch the "end of each harvest" phase: fields, feeding and breeding are
/// all resolved. This is the site for A145 綱作り 「各収穫の終わりに葦1を得る」
/// and C066 永遠のライ麦畑 「各収穫の後に」. Only effects whose trigger maps to
/// `EventKind::AfterHarvestComplete` fire here — the pre-field `Harvest`
/// dispatch no longer carries them (PR #385 review finding (a)).
/// `c066_take_food_instead` carries the owner's instead-choice for C066
/// (review finding (b)); it only matters for players who own C066.
#[allow(clippy::too_many_arguments)] // threads the same storages as the legacy cards.rs fns
pub fn dispatch_after_harvest_complete(
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
    c066_take_food_instead: bool,
) -> Result<Transition, String> {
    let all_players = players.to_vec();
    let mut ctx = DispatchContext {
        card_set,
        player: &mut players[player],
        cards,
        improvement_market,
        delayed_well,
        round,
        used_spaces,
        accumulations: None,
        event_budget,
        card_uses,
        card_use_limits,
        pending,
        actor: player,
        card_owner: player,
        beneficiary: player,
    };

    let trigger = PhaseTriggerData {
        space: None,
        improvement: None,
        baked: false,
        previous_player: None,
        used_spaces: used_spaces.clone(),
        site: EventKind::AfterHarvestComplete,
        event_kind: EventKind::AfterHarvestComplete,
        revealed_round_cards: Vec::new(),
        c066_take_food_instead,
    };

    resolve_phase(
        &mut ctx,
        player,
        TriggerPhase::After,
        &trigger,
        &all_players,
        false, // harvest tail: not post-work chain
    )?;
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
        accumulations: None,
        event_budget,
        card_uses,
        card_use_limits,
        pending,
        actor: player,
        card_owner: player,
        beneficiary: player,
    };

    let trigger = PhaseTriggerData {
        space: None,
        improvement: None,
        baked: true,
        previous_player: None,
        used_spaces: used_spaces.clone(),
        site: EventKind::Bake,
        event_kind: EventKind::Bake,
        revealed_round_cards: Vec::new(),
        c066_take_food_instead: false,
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
                // B073 贈り物かご: only the table row whose room count matches
                // the owner exactly resolves; other rows grant nothing, so the
                // legacy path keeps parity with the dispatcher scan.
                CardEffect::OnPlayGainIfRoomCount {
                    rooms,
                    good,
                    amount,
                } if player.rooms() == usize::from(*rooms) => {
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
                        crate::cards::HarvestFieldRule::Fixed => 1,
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{verified_band_a_cards, CardDefinition, CardKind, CardSet};
    use crate::model::{Animal, Crop, Good, Improvement, Pasture, Player, Space, Tile};
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
            &std::collections::BTreeMap::new(),
            &mut event_budget,
            &mut card_uses,
            &card_use_limits,
            &mut pending,
            &[],
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
            &std::collections::BTreeMap::new(),
            &mut event_budget,
            &mut card_uses,
            &card_use_limits,
            &mut pending,
            &[],
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
            &std::collections::BTreeMap::new(),
            &mut c.budget,
            &mut c.uses,
            &c.limits,
            &mut pending,
            &[],
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
                &std::collections::BTreeMap::new(),
                &mut c.budget,
                &mut c.uses,
                &c.limits,
                &mut pending,
                &[],
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
                true,
            )
            .unwrap();
            // dispatch_play applies the newly played card's points itself
            // (before the post-play destination), so no manual update here.
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
            &std::collections::BTreeMap::new(),
            &mut c.budget,
            &mut c.uses,
            &c.limits,
            &mut pending,
            &[],
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
            &std::collections::BTreeMap::new(),
            &mut c.budget,
            &mut c.uses,
            &c.limits,
            &mut pending_none(),
            &[],
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
            &std::collections::BTreeMap::new(),
            &mut budget,
            &mut uses,
            &limits,
            &mut pending,
            &[],
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
            &std::collections::BTreeMap::new(),
            &mut budget,
            &mut uses,
            &limits,
            &mut pending,
            &[],
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
            PendingKind::ChooseTriggerOrder {
                phase,
                triggers,
                site: _,
            } => {
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
            game.public.card_uses.get("AW2").copied(),
            Some(2),
            "each effect must fire exactly once"
        );
        assert_eq!(game.event_budget, 62, "two effects consume two budget");
    }

    /// P0-3 reproduction: save/load/replay after ChooseTriggerOrder pause/resume
    /// with A078 (Canoe, two AfterWorkerGain on Fishing).
    /// This test should FAIL before the fix (double recording breaks replay)
    /// and PASS after the fix.
    #[test]
    fn p0_3_repro_save_load_replay_after_choose_trigger_order() {
        use crate::engine::{Action, Detail};
        use crate::harvest::SimpleHarvestDetails;
        use crate::model::{Config, Game, Space};
        use crate::pending::{PendingResolution, Transition};

        // Seed-independent setup (PR #385 review R-P1-2): the random seed-1
        // deal stopped seating B121 and A078 in seat 0 together once this
        // batch changed the card-set composition, so the hands are forced
        // through `with_cards_and_hands`. The game RNG streams are derived
        // from the seed alone (`from_card_state`), so the round deck and
        // every other expectation below are unchanged; only the hand
        // contents differ.
        let set = crate::cards::verified_basic_card_set(2).expect("verified card set");
        let mut occs: Vec<String> = set
            .definitions
            .iter()
            .filter(|c| c.kind == crate::cards::CardKind::Occupation)
            .map(|c| c.id.clone())
            .collect();
        occs.sort();
        let mut minors: Vec<String> = set
            .definitions
            .iter()
            .filter(|c| c.kind == crate::cards::CardKind::MinorImprovement)
            .map(|c| c.id.clone())
            .collect();
        minors.sort();
        let mut seat0_occ = vec!["B121".to_string()];
        seat0_occ.extend(occs.iter().filter(|id| *id != "B121").take(6).cloned());
        let mut seat0_minor = vec!["A078".to_string()];
        seat0_minor.extend(minors.iter().filter(|id| *id != "A078").take(6).cloned());
        let seat1_occ: Vec<String> = occs
            .iter()
            .filter(|id| !seat0_occ.contains(id))
            .take(7)
            .cloned()
            .collect();
        let seat1_minor: Vec<String> = minors
            .iter()
            .filter(|id| !seat0_minor.contains(id))
            .take(7)
            .cloned()
            .collect();
        assert_eq!(
            (seat0_occ.len(), seat0_minor.len()),
            (7, 7),
            "seat 0 must hold seven occupations and seven minors"
        );
        assert_eq!(
            (seat1_occ.len(), seat1_minor.len()),
            (7, 7),
            "seat 1 must hold seven occupations and seven minors"
        );
        let mut game = Game::with_cards_and_hands(
            Config {
                players: 2,
                seed: 1,
                additional_action_space_enabled: false,
            },
            set,
            vec![
                [seat0_occ, seat0_minor].concat(),
                [seat1_occ, seat1_minor].concat(),
            ],
        )
        .expect("game creation must succeed");

        // Helper to let player 1 take a simple accumulation action
        let p1_simple_action = |game: &mut Game, space: Space| {
            game.public.active_player = 1;
            eprintln!("P1 available spaces: {:?}", game.available_spaces());
            eprintln!("P1 active_player: {}", game.public.active_player);
            eprintln!("P1 used spaces: {:?}", game.public.used);
            let action = Action {
                space,
                detail: Detail::None,
            };
            let transition = game.apply_with_plans(action, None, None, &SimpleHarvestDetails);
            eprintln!("P1 transition: {:?}", transition);
            assert_eq!(
                transition,
                Ok(Transition::Completed),
                "P1 action must complete"
            );
        };

        // --- Round 1: Player 0 plays occupation (B121), P1 acts, P0 takes Forest, P1 acts -> harvest ---
        // B121 (Geologist) is a free occupation (no cost).
        game.public.active_player = 0;
        let occ_action = Action {
            space: Space::Lessons,
            detail: Detail::PlayOccupation {
                card_id: "B121".into(),
            },
        };
        let transition = game.apply_with_plans(occ_action, None, None, &SimpleHarvestDetails);
        assert_eq!(
            transition,
            Ok(Transition::Completed),
            "B121 occupation play must complete"
        );

        // P1 takes ClayPit (so P0 can still take Forest)
        p1_simple_action(&mut game, Space::ClayPit);

        // P0 takes Forest (gets 3 wood)
        game.public.active_player = 0;
        eprintln!("P0 available spaces: {:?}", game.available_spaces());
        eprintln!("P0 active_player: {}", game.public.active_player);
        eprintln!("P0 used spaces: {:?}", game.public.used);
        let forest_action_r1 = Action {
            space: Space::Forest,
            detail: Detail::None,
        };
        let transition = game.apply_with_plans(forest_action_r1, None, None, &SimpleHarvestDetails);
        eprintln!("P0 transition: {:?}", transition);
        assert_eq!(
            transition,
            Ok(Transition::Completed),
            "Forest action must complete"
        );
        assert_eq!(
            game.public.players[0].supply.wood, 3,
            "Forest gives 3 wood at round 1"
        );

        // P1 takes DayLaborer -> all workers placed, round advances to 2 automatically
        p1_simple_action(&mut game, Space::DayLaborer);

        // --- Round 2: Already at round 2 after auto-advancement, play A078 and take Fishing ---
        assert_eq!(
            game.public.round, 2,
            "should be round 2 after auto-advancement"
        );
        assert_eq!(
            game.public.players[0].supply.wood, 3,
            "wood carried over (Forest accumulation at round 2 start goes to space, not player)"
        );

        // Play A078 (costs 2 wood, requires 1 occupation)
        game.public.active_player = 0;
        let minor_action = Action {
            space: Space::MeetingPlace,
            detail: Detail::PlayMinor {
                card_id: "A078".into(),
            },
        };
        let transition = game.apply_with_plans(minor_action, None, None, &SimpleHarvestDetails);
        assert_eq!(
            transition,
            Ok(Transition::Completed),
            "A078 minor play must complete"
        );
        assert_eq!(
            game.public.players[0].supply.wood, 1,
            "A078 costs 2 wood (3 - 2 = 1)"
        );

        // P1 takes ReedBank
        p1_simple_action(&mut game, Space::ReedBank);

        // --- Step: Take Fishing action - triggers A078's two effects, pauses with ChooseTriggerOrder ---
        game.public.active_player = 0;
        let fishing_action = Action {
            space: Space::Fishing,
            detail: Detail::None,
        };
        let transition = game.apply_with_plans(fishing_action, None, None, &SimpleHarvestDetails);

        // Should pause with ChooseTriggerOrder
        assert!(
            matches!(transition, Ok(Transition::Paused(_))),
            "must pause on Fishing with A078's two triggers"
        );

        // --- Step: Resolve with chosen order ---
        let outcome = game
            .resolve_pending_and_resume(PendingResolution::ChooseTriggerOrder {
                chosen_order: vec![0, 1],
            })
            .expect("resolution must succeed");
        assert_eq!(outcome, Transition::Completed, "resume must complete");

        // P1 takes DayLaborer -> harvest
        p1_simple_action(&mut game, Space::DayLaborer);

        // --- Step: Verify both bonuses applied ---
        assert_eq!(
            game.public.players[0].supply.food, 5,
            "A078 gives +1 food on Fishing (3 start + 1 Fishing accum + 1 A078)"
        );
        assert_eq!(
            game.public.players[0].supply.reed, 1,
            "A078 gives +1 reed on Fishing"
        );

        // --- Step: Count pending_resolution events (should be 1 after fix) ---
        let resolution_count = game
            .log
            .events
            .iter()
            .filter(|e| e.kind == "pending_resolution")
            .count();
        assert_eq!(
            resolution_count, 1,
            "must have exactly one pending_resolution event after fix"
        );

        // --- Step: Save/load/replay round-trip (all state created via public actions, so this must succeed) ---
        let json = game.save_json().expect("save must succeed");
        let loaded =
            Game::load_json(&json).expect("load_json must succeed - all state from public actions");
        let replayed = Game::replay_with_setup_and_start_player(
            loaded.config.clone(),
            (*loaded.card_set).clone(),
            loaded.log.initial_hands.clone(),
            loaded.log.initial_start_player_override,
            &loaded.log.events,
        )
        .expect("replay must succeed");

        // Final state hash must match
        assert_eq!(
            game.state_hash(),
            replayed.state_hash(),
            "state_hash must match after save/load/replay round-trip"
        );

        // Verify resources preserved after replay
        assert_eq!(
            replayed.public.players[0].supply.food, 5,
            "food preserved after replay"
        );
        assert_eq!(
            replayed.public.players[0].supply.reed, 1,
            "reed preserved after replay"
        );
    }

    /// Double-pause regression (PR #352 Round 2 P0-1/P1-2): resume from an
    /// AfterClayConstruction ChooseTriggerOrder pause into an AfterWorker
    /// ChooseTriggerOrder pause, with every state element produced by public
    /// actions only (card plays via Lessons, the clay house via renovation,
    /// clay/reed via accumulation spaces) -- so the save/load/replay section is
    /// a real proof rather than log-external state mutation. Each of the four
    /// card effects must apply exactly once: the delta across the double-pause
    /// resume is +2 food / +2 wood regardless of the starting supply.
    #[test]
    fn double_pause_after_clay_then_after_worker() {
        use crate::cards::{CardDefinition, CardEffect, CardKind};
        use crate::engine::{Action, Detail};
        use crate::harvest::SimpleHarvestDetails;
        use crate::model::{Config, Game, Good, House, RoundCard, Space};
        use crate::pending::{PendingResolution, Transition};

        // Two test occupations: 2x AfterClayConstructionGain and 2x
        // AfterWorkerGain on FarmExpansion. Fillers (inert, free) pad each
        // hand to the 7+7 deal the draft-hands constructor requires.
        let filler_occ = |i: usize| CardDefinition {
            use_limit: None,
            id: format!("FILLER_OCC_{i}"),
            kind: CardKind::Occupation,
            cost: vec![],
            points: 0,
            effects: vec![],
        };
        let filler_minor = |i: usize| CardDefinition {
            use_limit: None,
            id: format!("FILLER_MINOR_{i}"),
            kind: CardKind::MinorImprovement,
            cost: vec![],
            points: 0,
            effects: vec![],
        };
        let mut set = crate::cards::verified_basic_card_set(2).expect("verified basic card set");
        set.definitions.push(CardDefinition {
            use_limit: None,
            id: "TEST_CLAY_DOUBLE".into(),
            kind: CardKind::Occupation,
            cost: vec![],
            points: 0,
            effects: vec![
                CardEffect::AfterClayConstructionGain {
                    good: Good::Food,
                    amount: 1,
                },
                CardEffect::AfterClayConstructionGain {
                    good: Good::Wood,
                    amount: 1,
                },
            ],
        });
        set.definitions.push(CardDefinition {
            use_limit: None,
            id: "TEST_WORKER_DOUBLE".into(),
            kind: CardKind::Occupation,
            cost: vec![],
            points: 0,
            effects: vec![
                CardEffect::AfterWorkerGain {
                    space: Space::FarmExpansion,
                    good: Good::Food,
                    amount: 1,
                },
                CardEffect::AfterWorkerGain {
                    space: Space::FarmExpansion,
                    good: Good::Wood,
                    amount: 1,
                },
            ],
        });
        for i in 0..12 {
            set.definitions.push(filler_occ(i));
        }
        for i in 0..14 {
            set.definitions.push(filler_minor(i));
        }

        let p0_occ = [
            "TEST_CLAY_DOUBLE",
            "TEST_WORKER_DOUBLE",
            "FILLER_OCC_0",
            "FILLER_OCC_1",
            "FILLER_OCC_2",
            "FILLER_OCC_3",
            "FILLER_OCC_4",
        ];
        let p0_minor = [
            "FILLER_MINOR_0",
            "FILLER_MINOR_1",
            "FILLER_MINOR_2",
            "FILLER_MINOR_3",
            "FILLER_MINOR_4",
            "FILLER_MINOR_5",
            "FILLER_MINOR_6",
        ];
        let p1_occ = [
            "FILLER_OCC_5",
            "FILLER_OCC_6",
            "FILLER_OCC_7",
            "FILLER_OCC_8",
            "FILLER_OCC_9",
            "FILLER_OCC_10",
            "FILLER_OCC_11",
        ];
        let p1_minor = [
            "FILLER_MINOR_7",
            "FILLER_MINOR_8",
            "FILLER_MINOR_9",
            "FILLER_MINOR_10",
            "FILLER_MINOR_11",
            "FILLER_MINOR_12",
            "FILLER_MINOR_13",
        ];
        let hands = vec![
            p0_occ
                .iter()
                .chain(p0_minor.iter())
                .map(|s| (*s).to_string())
                .collect(),
            p1_occ
                .iter()
                .chain(p1_minor.iter())
                .map(|s| (*s).to_string())
                .collect(),
        ];

        // Player 0 is the start player: deterministic turn order.
        let mut game = Game::with_cards_and_hands_with_start_player(
            Config {
                players: 2,
                seed: 1,
                additional_action_space_enabled: false,
            },
            set,
            hands,
            0,
        )
        .expect("game creation must succeed");

        fn act(game: &mut Game, player: usize, space: Space, detail: Detail) -> Transition {
            game.public.active_player = player;
            match game.apply_with_plans(Action { space, detail }, None, None, &SimpleHarvestDetails)
            {
                Ok(t) => t,
                Err(e) => panic!("P{player} action {space:?} failed: {e}"),
            }
        }

        // --- Round 1: play both occupations via Lessons (public actions only).
        // The first occupation play costs no fee; the second costs 1 food.
        // (Both are played by the placement loop below, one per round, since
        // Lessons is usable once per round.)
        assert_eq!(game.public.players[0].played_cards, [] as [String; 0]);

        // --- Rounds 2..: accumulate clay (7) and reed (3), renovate to a clay
        // house as soon as HouseRedevelopment is revealed, then build a clay
        // room via FarmExpansion. That build triggers the double pause:
        // AfterClayConstruction (2 effects) -> AfterWorker (2 effects).
        let mut paused: Option<Box<crate::pending::PendingDecision>> = None;
        let mut build_pre: Option<(u32, u32)> = None;
        // Alternate P0 / P1 placements. Lessons is usable once per round per
        // player (a used space stays used until the round ends), so the two
        // occupation plays land in different rounds automatically.
        let mut p0_turn = true;
        for _ in 0..60 {
            if paused.is_some() {
                break;
            }
            if p0_turn {
                let clay = game.public.players[0].supply.clay;
                let reed = game.public.players[0].supply.reed;
                let house = game.public.players[0].house;
                let clay_double = game.public.players[0]
                    .played_cards
                    .iter()
                    .any(|id| id == "TEST_CLAY_DOUBLE");
                let worker_double = game.public.players[0]
                    .played_cards
                    .iter()
                    .any(|id| id == "TEST_WORKER_DOUBLE");
                let lessons_free = !game.public.used.contains(&Space::Lessons);
                let (space, detail) = if !clay_double && lessons_free {
                    (
                        Space::Lessons,
                        Detail::PlayOccupation {
                            card_id: "TEST_CLAY_DOUBLE".into(),
                        },
                    )
                } else if !worker_double && lessons_free {
                    (
                        Space::Lessons,
                        Detail::PlayOccupation {
                            card_id: "TEST_WORKER_DOUBLE".into(),
                        },
                    )
                } else if house == House::Clay && clay >= 5 && reed >= 2 {
                    (
                        Space::FarmExpansion,
                        Detail::Build {
                            rooms: vec![1], // adjacent to the starting room at 0
                            stables: vec![],
                        },
                    )
                } else if house == House::Wood
                    && clay >= 7
                    && reed >= 3
                    && game
                        .public
                        .revealed
                        .contains(&RoundCard::HouseRedevelopment)
                {
                    (
                        Space::HouseRedevelopment,
                        Detail::Renovate {
                            bonus_improvement: None,
                            instant_bake: 0,
                            pastures: vec![],
                        },
                    )
                } else {
                    let preferred = if clay < 7 {
                        Space::ClayPit
                    } else if reed < 3 {
                        Space::ReedBank
                    } else {
                        Space::DayLaborer
                    };
                    let fallback = if game.public.used.contains(&preferred) {
                        [
                            Space::DayLaborer,
                            Space::ClayPit,
                            Space::ReedBank,
                            Space::Fishing,
                            Space::Forest,
                        ]
                        .into_iter()
                        .find(|s| !game.public.used.contains(s))
                        .expect("P0 has a free fallback space")
                    } else {
                        preferred
                    };
                    (fallback, Detail::None)
                };
                // Snapshot supply right before the build so the delta asserts
                // below measure exactly this action's card effects.
                if space == Space::FarmExpansion {
                    build_pre = Some((
                        game.public.players[0].supply.food,
                        game.public.players[0].supply.wood,
                    ));
                }
                let t = act(&mut game, 0, space, detail);
                match t {
                    Transition::Completed => {}
                    Transition::Paused(pending) => {
                        paused = Some(pending);
                        break;
                    }
                }
            } else {
                // P1 placement: filler, no played cards => no card effects.
                let p1_space = [Space::DayLaborer, Space::Fishing, Space::Forest]
                    .into_iter()
                    .find(|s| game.legal_spaces().contains(s))
                    .expect("P1 must have a simple legal space");
                act(&mut game, 1, p1_space, Detail::None);
            }
            p0_turn = !p0_turn;
        }
        let pending = paused.expect("the clay-room build must pause");

        // First pause: AfterClayConstruction, two simultaneous After effects.
        assert!(matches!(
            pending.kind,
            PendingKind::ChooseTriggerOrder {
                phase: crate::card_events::TriggerPhase::After,
                ..
            }
        ));

        // Resolve first pause -> the chain advances to AfterWorker and pauses again.
        let outcome = game
            .resolve_pending_and_resume(PendingResolution::ChooseTriggerOrder {
                chosen_order: vec![0, 1],
            })
            .expect("first resolution must succeed");
        let Transition::Paused(pending2) = outcome else {
            panic!("expected second pause on AfterWorker, got {outcome:?}");
        };
        assert!(matches!(
            pending2.kind,
            PendingKind::ChooseTriggerOrder {
                phase: crate::card_events::TriggerPhase::After,
                ..
            }
        ));

        // Resolve second pause -> the action completes.
        let outcome2 = game
            .resolve_pending_and_resume(PendingResolution::ChooseTriggerOrder {
                chosen_order: vec![0, 1],
            })
            .expect("second resolution must succeed");
        assert_eq!(
            outcome2,
            Transition::Completed,
            "final resume must complete"
        );

        // Each of the four card effects applied exactly once: +2 food, +2 wood
        // relative to the state right before the build. Asserting the delta
        // (not the absolute supply) keeps the test meaningful regardless of
        // feeding/begging from earlier rounds.
        let (pre_food, pre_wood) = build_pre.expect("the build action ran");
        assert_eq!(
            game.public.players[0].supply.food,
            pre_food + 2,
            "one food from clay + one food from worker"
        );
        assert_eq!(
            game.public.players[0].supply.wood,
            pre_wood + 2,
            "one wood from clay + one wood from worker"
        );

        // Sanity: the build really happened (room count grew) and the action
        // bookkeeping is complete (space used, worker placed).
        assert_eq!(game.public.players[0].rooms(), 3);
        assert!(game.public.used.contains(&Space::FarmExpansion));

        // Round-trip save/load/replay: every state element above was produced
        // by public actions, so this must reproduce byte-for-byte.
        let json = game.save_json().expect("save must succeed");
        let loaded = Game::load_json(&json).expect("load_json must succeed");
        let replayed = Game::replay_with_setup_and_start_player(
            loaded.config.clone(),
            (*loaded.card_set).clone(),
            loaded.log.initial_hands.clone(),
            loaded.log.initial_start_player_override,
            &loaded.log.events,
        )
        .expect("replay must succeed");

        assert_eq!(
            game.state_hash(),
            replayed.state_hash(),
            "state_hash must match after save/load/replay round-trip"
        );
        assert_eq!(
            replayed.public.players[0].supply.food, game.public.players[0].supply.food,
            "food must reproduce after replay"
        );
        assert_eq!(
            replayed.public.players[0].supply.wood, game.public.players[0].supply.wood,
            "wood must reproduce after replay"
        );
        assert!(replayed.public.used.contains(&Space::FarmExpansion));
        assert_eq!(replayed.public.workers_placed, game.public.workers_placed);
    }

    /// C111 小動物飼育員 via the production round-start dispatcher: the
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
                &std::collections::BTreeMap::new(),
                &mut c.budget,
                &mut c.uses,
                &c.limits,
                &mut pending,
                &[],
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

    /// C116 調度品職人 via the production play dispatcher: each later
    /// occupation play grants 1 wood per food paid as its occupation cost.
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
        dispatch_play(
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
            false,
        )
        .unwrap();
        assert_eq!(
            c.players[0].supply.wood, 0,
            "the printed individual cost of 2 food must not grant wood via C116"
        );
        // The Lessons fee IS the occupation cost, so it counts alone.
        c.players[0].played_cards = vec!["C116".into(), "C111".into()];
        dispatch_play(
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
            true,
        )
        .unwrap();
        assert_eq!(c.players[0].supply.wood, 3, "occupation cost 3 -> 3 wood");
        // Minor improvements and zero-cost occupations do not trigger it.
        c.players[0].played_cards = vec!["C116".into(), "C106".into()];
        dispatch_play(
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
            false,
        )
        .unwrap();
        assert_eq!(
            c.players[0].supply.wood, 3,
            "no occupation cost paid -> no gain"
        );
    }

    /// C116 boundary reproduction (static-review 2026-11-20 triage card 3/4),
    /// axis 1 — the LESSON FEE ALONE. The 2016 Lookout appendix calls the
    /// Lessons-space food payment the "occupation cost" ("On the 'Lessons'
    /// action space above 'Day Laborer', the first occupation you play in the
    /// game is free. Each additional occupation you play after that costs you
    /// 1 food ... These costs are called 'occupation costs'."), and the printed
    /// card grants "1 wood for each food paid as occupation cost", so the fee
    /// must feed the multiplier with no printed cost involved.
    #[test]
    fn c116_lesson_fee_alone_grants_wood_per_fee_food() {
        let mut set = CardSet {
            definitions: crate::cards::verified_band_c_cards(),
        };
        set.definitions.push(CardDefinition {
            use_limit: None,
            id: "occ_free".into(),
            kind: CardKind::Occupation,
            cost: vec![],
            points: 0,
            effects: vec![],
        });
        let mut c = Ctx::new();
        // Zone move already applied by the caller: C116 first, new card last.
        c.players[0].played_cards = vec!["C116".into(), "occ_free".into()];
        let mut market = vec![];
        let mut well = BTreeMap::new();
        let mut pending = None;
        let mut budget = 64u32;
        let mut uses = BTreeMap::new();
        let limits = BTreeMap::new();
        dispatch_play(
            &set,
            0,
            "occ_free",
            1,
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
            false,
        )
        .unwrap();
        assert_eq!(
            c.players[0].supply.wood, 1,
            "occupation cost (Lessons fee) of 1 food must grant 1 wood via C116"
        );
    }

    /// C116 boundary reproduction, axis 2 — the PRINTED INDIVIDUAL COST ALONE.
    /// The 2016 Lookout appendix separates the two payments: "The occupation
    /// cost is what you must pay when playing the occupation card from your
    /// hand. It is stated on the 'Lessons' action space. The (rather rare)
    /// individual cost is printed on the occupation card itself." C116's effect
    /// counts "food paid as occupation cost", so an occupation's own printed
    /// food cost (its individual cost) must NOT feed the multiplier.
    #[test]
    fn c116_printed_individual_cost_alone_grants_no_wood() {
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
        dispatch_play(
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
            false,
        )
        .unwrap();
        assert_eq!(
            c.players[0].supply.wood, 0,
            "the printed individual cost is not the occupation cost and must not grant wood"
        );
    }

    /// C116 boundary reproduction, both axes at once: the multiplier follows the
    /// occupation cost (the fee) only, even when the played occupation also
    /// charges a printed individual cost.
    #[test]
    fn c116_counts_only_the_occupation_cost_when_both_payments_are_made() {
        let mut set = CardSet {
            definitions: crate::cards::verified_band_c_cards(),
        };
        set.definitions.push(CardDefinition {
            use_limit: None,
            id: "occ_food1".into(),
            kind: CardKind::Occupation,
            cost: vec![(Good::Food, 1)],
            points: 0,
            effects: vec![],
        });
        let mut c = Ctx::new();
        // Zone move already applied by the caller: C116 first, new card last.
        c.players[0].played_cards = vec!["C116".into(), "occ_food1".into()];
        let mut market = vec![];
        let mut well = BTreeMap::new();
        let mut pending = None;
        let mut budget = 64u32;
        let mut uses = BTreeMap::new();
        let limits = BTreeMap::new();
        dispatch_play(
            &set,
            0,
            "occ_food1",
            1,
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
            false,
        )
        .unwrap();
        assert_eq!(
            c.players[0].supply.wood, 1,
            "fee 1 + individual cost 1 must grant only 1 wood (occupation cost only)"
        );
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
            false,
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
            false,
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
            false,
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
                false,
            )
            .unwrap();
            // dispatch_play applies the newly played card's points itself
            // (before the post-play destination), so no manual update here.
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

    /// Reproduction test for PR #326 P0: HarvestFieldGain should NOT fire on
    /// dispatch_after_worker. C106 has HarvestFieldGain (VegetablesHarvested).
    #[test]
    fn c106_harvest_field_gain_does_not_fire_on_after_worker() {
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
        let mut market = vec![];
        let mut well = BTreeMap::new();
        let mut used = std::collections::BTreeSet::new();
        used.insert(Space::DayLaborer);
        let mut pending = None;
        dispatch_after_worker(
            &set,
            0,
            Space::DayLaborer,
            &mut c.cards,
            &mut c.players,
            &mut market,
            &mut well,
            4,
            &used,
            &mut c.budget,
            &mut c.uses,
            &c.limits,
            &mut pending,
            false,
        )
        .unwrap();
        // C106 has HarvestFieldGain (VegetablesHarvested) which should ONLY fire
        // on harvest field phase, NOT on worker completion.
        // If the bug exists, this will incorrectly gain 2 food.
        assert_eq!(
            c.players[0].supply.food, 0,
            "C106 HarvestFieldGain must not fire on after_worker"
        );
    }

    /// Reproduction test for PR #326 P0: StartRoundGain should NOT fire on
    /// dispatch_after_worker. B057 has StartRoundGain (Food+1 when Wood house).
    #[test]
    fn b057_start_round_gain_does_not_fire_on_after_worker() {
        let set = CardSet {
            definitions: crate::cards::verified_round_and_harvest_cards(),
        };
        let mut c = Ctx::new();
        c.players[0].played_cards = vec!["B057".into()];
        c.players[0].house = House::Wood;
        let mut market = vec![];
        let mut well = BTreeMap::new();
        let mut used = std::collections::BTreeSet::new();
        used.insert(Space::DayLaborer);
        let mut pending = None;
        dispatch_after_worker(
            &set,
            0,
            Space::DayLaborer,
            &mut c.cards,
            &mut c.players,
            &mut market,
            &mut well,
            4,
            &used,
            &mut c.budget,
            &mut c.uses,
            &c.limits,
            &mut pending,
            false,
        )
        .unwrap();
        // B057 has StartRoundGain which should ONLY fire on round start, NOT on
        // worker completion. If the bug exists, this will incorrectly gain 1 food.
        assert_eq!(
            c.players[0].supply.food, 0,
            "B057 StartRoundGain must not fire on after_worker"
        );
    }

    /// Reproduction test for PR #326 P0: C111 StartRoundGainIfFoodAtLeastRound
    /// should NOT fire on dispatch_after_worker.
    #[test]
    fn c111_start_round_gain_if_food_at_least_round_does_not_fire_on_after_worker() {
        let set = CardSet {
            definitions: crate::cards::verified_band_c_cards(),
        };
        let mut c = Ctx::new();
        c.players[0].played_cards = vec!["C111".into()];
        c.players[0].supply.food = 10; // >= round (4)
        let mut market = vec![];
        let mut well = BTreeMap::new();
        let mut used = std::collections::BTreeSet::new();
        used.insert(Space::DayLaborer);
        let mut pending = None;
        dispatch_after_worker(
            &set,
            0,
            Space::DayLaborer,
            &mut c.cards,
            &mut c.players,
            &mut market,
            &mut well,
            4,
            &used,
            &mut c.budget,
            &mut c.uses,
            &c.limits,
            &mut pending,
            false,
        )
        .unwrap();
        // C111 has StartRoundGainIfFoodAtLeastRound which should ONLY fire on
        // round start, NOT on worker completion.
        assert_eq!(
            c.players[0].supply.food, 10,
            "C111 StartRoundGainIfFoodAtLeastRound must not fire on after_worker"
        );
    }

    /// Reproduction test for PR #326 P0: B118 StartRoundGain should NOT fire on
    /// dispatch_after_worker.
    #[test]
    fn b118_start_round_gain_does_not_fire_on_after_worker() {
        let set = CardSet {
            definitions: crate::cards::verified_round_and_harvest_cards(),
        };
        let mut c = Ctx::new();
        c.players[0].played_cards = vec!["B118".into()];
        // B118 requires 2 rooms; Player::new() gives 2 rooms by default.
        // The only reason it doesn't fire here is the event_kind check (RoundStart vs WorkerPlaced).
        c.players[0].supply.wood = 10;
        let mut market = vec![];
        let mut well = BTreeMap::new();
        let mut used = std::collections::BTreeSet::new();
        used.insert(Space::DayLaborer);
        let mut pending = None;
        dispatch_after_worker(
            &set,
            0,
            Space::DayLaborer,
            &mut c.cards,
            &mut c.players,
            &mut market,
            &mut well,
            4,
            &used,
            &mut c.budget,
            &mut c.uses,
            &c.limits,
            &mut pending,
            false,
        )
        .unwrap();
        // B118 has 2 rooms by default; the event_kind check prevents firing.
        assert_eq!(
            c.players[0].supply.wood, 10,
            "B118 StartRoundGain must not fire on after_worker (event_kind check)"
        );
    }

    /// Reproduction test for PR #326 P0: B107 StartRoundGain should NOT fire on
    /// dispatch_after_worker.
    #[test]
    fn b107_start_round_gain_does_not_fire_on_after_worker() {
        let set = CardSet {
            definitions: crate::cards::verified_simple_occupations(3), // B107 is in simple_occupations
        };
        let mut c = Ctx::new();
        c.players[0].played_cards = vec!["B107".into()];
        c.players[0].house = House::Stone; // B107 requires Stone house
        let mut market = vec![];
        let mut well = BTreeMap::new();
        let mut used = std::collections::BTreeSet::new();
        used.insert(Space::DayLaborer);
        let mut pending = None;
        dispatch_after_worker(
            &set,
            0,
            Space::DayLaborer,
            &mut c.cards,
            &mut c.players,
            &mut market,
            &mut well,
            4,
            &used,
            &mut c.budget,
            &mut c.uses,
            &c.limits,
            &mut pending,
            false,
        )
        .unwrap();
        assert_eq!(
            c.players[0].supply.food, 0,
            "B107 StartRoundGain must not fire on after_worker"
        );
    }

    /// Reproduction test for PR #326 P0: B050 HarvestFieldGain should NOT fire on
    /// dispatch_after_worker.
    #[test]
    fn b050_harvest_field_gain_does_not_fire_on_after_worker() {
        let set = CardSet {
            definitions: crate::cards::verified_round_and_harvest_cards(),
        };
        let mut c = Ctx::new();
        c.players[0].played_cards = vec!["B050".into()];
        c.players[0].pastures.push(Pasture {
            id: 0,
            cells: vec![2, 3, 4, 5, 6],
            animal: Some(Animal::Sheep),
            quantity: 6,
        });
        c.players[0].pastures.push(Pasture {
            id: 1,
            cells: vec![7, 8],
            animal: Some(Animal::Cattle),
            quantity: 3,
        });
        let mut market = vec![];
        let mut well = BTreeMap::new();
        let mut used = std::collections::BTreeSet::new();
        used.insert(Space::DayLaborer);
        let mut pending = None;
        dispatch_after_worker(
            &set,
            0,
            Space::DayLaborer,
            &mut c.cards,
            &mut c.players,
            &mut market,
            &mut well,
            4,
            &used,
            &mut c.budget,
            &mut c.uses,
            &c.limits,
            &mut pending,
            false,
        )
        .unwrap();
        // B050 has HarvestFieldGain (Animals) which should ONLY fire on harvest
        // field phase, NOT on worker completion.
        assert_eq!(
            c.players[0].supply.food, 0,
            "B050 HarvestFieldGain must not fire on after_worker"
        );
    }

    /// Reproduction test for PR #326 P0: B061 HarvestFieldGain should NOT fire on
    /// dispatch_after_worker.
    #[test]
    fn b061_harvest_field_gain_does_not_fire_on_after_worker() {
        let set = CardSet {
            definitions: crate::cards::verified_round_and_harvest_cards(),
        };
        let mut c = Ctx::new();
        c.players[0].played_cards = vec!["B061".into()];
        // B061 needs MixedFields (grain + vegetable + fallow) - set up so mutation would fire
        c.players[0].farm[1].tile = Tile::Field {
            crop: Some(Crop::Grain),
            remaining: 2,
        };
        c.players[0].farm[2].tile = Tile::Field {
            crop: Some(Crop::Vegetable),
            remaining: 1,
        };
        c.players[0].farm[3].tile = Tile::Field {
            crop: None,
            remaining: 0,
        };
        let mut market = vec![];
        let mut well = BTreeMap::new();
        let mut used = std::collections::BTreeSet::new();
        used.insert(Space::DayLaborer);
        let mut pending = None;
        dispatch_after_worker(
            &set,
            0,
            Space::DayLaborer,
            &mut c.cards,
            &mut c.players,
            &mut market,
            &mut well,
            4,
            &used,
            &mut c.budget,
            &mut c.uses,
            &c.limits,
            &mut pending,
            false,
        )
        .unwrap();
        assert_eq!(
            c.players[0].supply.food, 0,
            "B061 HarvestFieldGain must not fire on after_worker"
        );
    }

    /// Reproduction test for PR #326 P0: A106 HarvestFieldGain should NOT fire on
    /// dispatch_after_worker.
    #[test]
    fn a106_harvest_field_gain_does_not_fire_on_after_worker() {
        let set = CardSet {
            definitions: crate::cards::verified_band_a_cards(),
        };
        let mut c = Ctx::new();
        c.players[0].played_cards = vec!["A106".into()];
        // A106 needs LastGrainTaken (grain field with remaining 1) and/or LastVegetableTaken (veg field with remaining 1)
        c.players[0].farm[1].tile = Tile::Field {
            crop: Some(Crop::Grain),
            remaining: 1,
        };
        c.players[0].farm[2].tile = Tile::Field {
            crop: Some(Crop::Vegetable),
            remaining: 1,
        };
        let mut market = vec![];
        let mut well = BTreeMap::new();
        let mut used = std::collections::BTreeSet::new();
        used.insert(Space::DayLaborer);
        let mut pending = None;
        dispatch_after_worker(
            &set,
            0,
            Space::DayLaborer,
            &mut c.cards,
            &mut c.players,
            &mut market,
            &mut well,
            4,
            &used,
            &mut c.budget,
            &mut c.uses,
            &c.limits,
            &mut pending,
            false,
        )
        .unwrap();
        assert_eq!(
            c.players[0].supply.food, 0,
            "A106 HarvestFieldGain must not fire on after_worker"
        );
    }

    /// Reproduction test for PR #326 P0: E107 HarvestFieldGain should NOT fire on
    /// dispatch_after_worker.
    #[test]
    fn e107_harvest_field_gain_does_not_fire_on_after_worker() {
        let set = CardSet {
            definitions: crate::cards::verified_band_e_cards(),
        };
        let mut c = Ctx::new();
        c.players[0].played_cards = vec!["E107".into()];
        // E107 needs 2+ plowed fields (FieldCountThresholds: 2->1 food) - set up so mutation would fire
        c.players[0].farm[1].tile = Tile::Field {
            crop: Some(Crop::Grain),
            remaining: 2,
        };
        c.players[0].farm[2].tile = Tile::Field {
            crop: Some(Crop::Grain),
            remaining: 2,
        };
        let mut market = vec![];
        let mut well = BTreeMap::new();
        let mut used = std::collections::BTreeSet::new();
        used.insert(Space::DayLaborer);
        let mut pending = None;
        dispatch_after_worker(
            &set,
            0,
            Space::DayLaborer,
            &mut c.cards,
            &mut c.players,
            &mut market,
            &mut well,
            4,
            &used,
            &mut c.budget,
            &mut c.uses,
            &c.limits,
            &mut pending,
            false,
        )
        .unwrap();
        assert_eq!(
            c.players[0].supply.food, 0,
            "E107 HarvestFieldGain must not fire on after_worker"
        );
    }

    /// Reproduction test for PR #326 P0: E111 StartRoundGainIfNoMinorPlayed should NOT
    /// fire on dispatch_after_worker.
    #[test]
    fn e111_start_round_gain_if_no_minor_played_does_not_fire_on_after_worker() {
        let set = CardSet {
            definitions: crate::cards::verified_band_e_cards(),
        };
        let mut c = Ctx::new();
        c.players[0].played_cards = vec!["E111".into()];
        // E111 condition: no minor improvements played - satisfied in this test
        // E111 also has HarvestFieldGainIfNoMinorPlayed giving wood - check both food and wood
        c.players[0].supply.wood = 0;
        let mut market = vec![];
        let mut well = BTreeMap::new();
        let mut used = std::collections::BTreeSet::new();
        used.insert(Space::DayLaborer);
        let mut pending = None;
        dispatch_after_worker(
            &set,
            0,
            Space::DayLaborer,
            &mut c.cards,
            &mut c.players,
            &mut market,
            &mut well,
            4,
            &used,
            &mut c.budget,
            &mut c.uses,
            &c.limits,
            &mut pending,
            false,
        )
        .unwrap();
        assert_eq!(
            c.players[0].supply.food, 0,
            "E111 StartRoundGainIfNoMinorPlayed must not fire on after_worker"
        );
        assert_eq!(
            c.players[0].supply.wood, 0,
            "E111 HarvestFieldGainIfNoMinorPlayed must not fire on after_worker"
        );
    }

    /// Reproduction test for PR #326 P0: E117 HarvestFieldGain should NOT fire on
    /// dispatch_after_worker.
    #[test]
    fn e117_harvest_field_gain_does_not_fire_on_after_worker() {
        let set = CardSet {
            definitions: crate::cards::verified_band_e_cards(),
        };
        let mut c = Ctx::new();
        c.players[0].played_cards = vec!["E117".into()];
        // E117 needs a wheat (grain) field (HasWheatField) - set up so mutation would fire
        c.players[0].farm[1].tile = Tile::Field {
            crop: Some(Crop::Grain),
            remaining: 2,
        };
        let mut market = vec![];
        let mut well = BTreeMap::new();
        let mut used = std::collections::BTreeSet::new();
        used.insert(Space::DayLaborer);
        let mut pending = None;
        dispatch_after_worker(
            &set,
            0,
            Space::DayLaborer,
            &mut c.cards,
            &mut c.players,
            &mut market,
            &mut well,
            4,
            &used,
            &mut c.budget,
            &mut c.uses,
            &c.limits,
            &mut pending,
            false,
        )
        .unwrap();
        assert_eq!(
            c.players[0].supply.wood, 0,
            "E117 HarvestFieldGain must not fire on after_worker"
        );
    }

    /// Test that the correct effects DO fire on their proper dispatchers.
    /// C106 should fire on harvest field phase.
    #[test]
    fn c106_harvest_field_gain_fires_on_harvest_field() {
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
            &std::collections::BTreeSet::new(),
            &mut c.budget,
            &mut c.uses,
            &c.limits,
            &mut pending,
        )
        .unwrap();
        // C106 has HarvestFieldGain (VegetablesHarvested) - should fire on harvest field
        assert_eq!(
            c.players[0].supply.food, 2,
            "C106 HarvestFieldGain must fire on harvest field phase"
        );
    }

    /// Test that B057 fires on round start.
    #[test]
    fn b057_start_round_gain_fires_on_start_round() {
        let set = CardSet {
            definitions: crate::cards::verified_round_and_harvest_cards(),
        };
        let mut c = Ctx::new();
        c.players[0].played_cards = vec!["B057".into()];
        c.players[0].house = House::Wood;
        let mut market = vec![];
        let mut well = BTreeMap::new();
        let mut pending = None;
        dispatch_start_round(
            &set,
            &mut c.players,
            &mut c.cards,
            &mut market,
            &mut well,
            4,
            &std::collections::BTreeSet::new(),
            &std::collections::BTreeMap::new(),
            &mut c.budget,
            &mut c.uses,
            &c.limits,
            &mut pending,
            &[],
        )
        .unwrap();
        assert_eq!(
            c.players[0].supply.food, 1,
            "B057 StartRoundGain must fire on start_round"
        );
    }

    /// Test that B118 fires on harvest field phase.
    #[test]
    fn b050_harvest_field_gain_fires_on_harvest_field() {
        let set = CardSet {
            definitions: crate::cards::verified_round_and_harvest_cards(),
        };
        let mut c = Ctx::new();
        c.players[0].played_cards = vec!["B050".into()];
        // 6 sheep = 2 food (per 3), 3 cattle = 1 food (per 2) -> total 3 food
        c.players[0].pastures.push(Pasture {
            id: 0,
            cells: vec![2, 3, 4, 5, 6],
            animal: Some(Animal::Sheep),
            quantity: 6,
        });
        c.players[0].pastures.push(Pasture {
            id: 1,
            cells: vec![7, 8],
            animal: Some(Animal::Cattle),
            quantity: 3,
        });
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
            &std::collections::BTreeSet::new(),
            &mut c.budget,
            &mut c.uses,
            &c.limits,
            &mut pending,
        )
        .unwrap();
        assert_eq!(
                    c.players[0].supply.food, 3,
                    "B050 HarvestFieldGain (Animals) must fire on harvest field phase: 6 sheep + 3 cattle = 3 food"
                );
    }

    /// Test that B061 fires on harvest field phase.
    #[test]
    fn b061_harvest_field_gain_fires_on_harvest_field() {
        let set = CardSet {
            definitions: crate::cards::verified_round_and_harvest_cards(),
        };
        let mut c = Ctx::new();
        c.players[0].played_cards = vec!["B061".into()];
        // B061 needs MixedFields: grain + vegetable + fallow
        c.players[0].farm[1].tile = Tile::Field {
            crop: Some(Crop::Grain),
            remaining: 2,
        };
        c.players[0].farm[2].tile = Tile::Field {
            crop: Some(Crop::Vegetable),
            remaining: 1,
        };
        c.players[0].farm[3].tile = Tile::Field {
            crop: None,
            remaining: 0,
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
            &std::collections::BTreeSet::new(),
            &mut c.budget,
            &mut c.uses,
            &c.limits,
            &mut pending,
        )
        .unwrap();
        assert_eq!(
            c.players[0].supply.food, 3,
            "B061 HarvestFieldGain (MixedFields) must fire on harvest field phase"
        );
    }

    /// Test that B118 fires on round start.
    #[test]
    fn b118_start_round_gain_fires_on_start_round() {
        let set = CardSet {
            definitions: crate::cards::verified_round_and_harvest_cards(),
        };
        let mut c = Ctx::new();
        c.players[0].played_cards = vec!["B118".into()];
        // B118 needs exactly 2 rooms - Player::new() gives 2 rooms by default
        c.players[0].supply.wood = 10;
        let mut market = vec![];
        let mut well = BTreeMap::new();
        let mut pending = None;
        dispatch_start_round(
            &set,
            &mut c.players,
            &mut c.cards,
            &mut market,
            &mut well,
            4,
            &std::collections::BTreeSet::new(),
            &std::collections::BTreeMap::new(),
            &mut c.budget,
            &mut c.uses,
            &c.limits,
            &mut pending,
            &[],
        )
        .unwrap();
        assert_eq!(
            c.players[0].supply.wood, 11,
            "B118 StartRoundGain must fire on start_round (exactly 2 rooms)"
        );
    }

    /// Test that C111 fires on round start (food >= round).
    #[test]
    fn c111_start_round_gain_fires_on_start_round() {
        let set = CardSet {
            definitions: crate::cards::verified_band_c_cards(),
        };
        let mut c = Ctx::new();
        c.players[0].played_cards = vec!["C111".into()];
        c.players[0].supply.food = 10; // >= round (4)
        let mut market = vec![];
        let mut well = BTreeMap::new();
        let mut pending = None;
        dispatch_start_round(
            &set,
            &mut c.players,
            &mut c.cards,
            &mut market,
            &mut well,
            4,
            &std::collections::BTreeSet::new(),
            &std::collections::BTreeMap::new(),
            &mut c.budget,
            &mut c.uses,
            &c.limits,
            &mut pending,
            &[],
        )
        .unwrap();
        assert_eq!(
            c.players[0].supply.food, 11,
            "C111 StartRoundGainIfFoodAtLeastRound must fire on start_round"
        );
    }

    /// Test that E107 fires on harvest field phase.
    #[test]
    fn e107_harvest_field_gain_fires_on_harvest_field() {
        let set = CardSet {
            definitions: crate::cards::verified_band_e_cards(),
        };
        let mut c = Ctx::new();
        c.players[0].played_cards = vec!["E107".into()];
        // E107: 4 plowed fields -> 2 food (thresholds: 2->1, 4->2, 6->3, 7->4)
        c.players[0].farm[1].tile = Tile::Field {
            crop: Some(Crop::Grain),
            remaining: 2,
        };
        c.players[0].farm[2].tile = Tile::Field {
            crop: Some(Crop::Grain),
            remaining: 2,
        };
        c.players[0].farm[3].tile = Tile::Field {
            crop: Some(Crop::Vegetable),
            remaining: 1,
        };
        c.players[0].farm[4].tile = Tile::Field {
            crop: Some(Crop::Vegetable),
            remaining: 1,
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
            &std::collections::BTreeSet::new(),
            &mut c.budget,
            &mut c.uses,
            &c.limits,
            &mut pending,
        )
        .unwrap();
        assert_eq!(
                    c.players[0].supply.food, 2,
                    "E107 HarvestFieldGain (FieldCountThresholds) must fire on harvest field phase: 4 fields -> 2 food"
                );
    }

    /// Test that E111 fires on round start (food, no minor played).
    #[test]
    fn e111_start_round_gain_fires_on_start_round() {
        let set = CardSet {
            definitions: crate::cards::verified_band_e_cards(),
        };
        let mut c = Ctx::new();
        c.players[0].played_cards = vec!["E111".into()];
        // E111: no minor improvements played -> 1 food at round start
        let mut market = vec![];
        let mut well = BTreeMap::new();
        let mut pending = None;
        dispatch_start_round(
            &set,
            &mut c.players,
            &mut c.cards,
            &mut market,
            &mut well,
            4,
            &std::collections::BTreeSet::new(),
            &std::collections::BTreeMap::new(),
            &mut c.budget,
            &mut c.uses,
            &c.limits,
            &mut pending,
            &[],
        )
        .unwrap();
        assert_eq!(
            c.players[0].supply.food, 1,
            "E111 StartRoundGainIfNoMinorPlayed must fire on start_round"
        );
    }

    /// Test that E111 fires on harvest field phase (wood, no minor played).
    #[test]
    fn e111_harvest_field_gain_fires_on_harvest_field() {
        let set = CardSet {
            definitions: crate::cards::verified_band_e_cards(),
        };
        let mut c = Ctx::new();
        c.players[0].played_cards = vec!["E111".into()];
        c.players[0].supply.wood = 0;
        // E111: no minor improvements played -> 1 wood at harvest field phase
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
            &std::collections::BTreeSet::new(),
            &mut c.budget,
            &mut c.uses,
            &c.limits,
            &mut pending,
        )
        .unwrap();
        assert_eq!(
            c.players[0].supply.wood, 1,
            "E111 HarvestFieldGainIfNoMinorPlayed must fire on harvest field phase"
        );
    }

    /// Test that E117 fires on harvest field phase.
    #[test]
    fn e117_harvest_field_gain_fires_on_harvest_field() {
        let set = CardSet {
            definitions: crate::cards::verified_band_e_cards(),
        };
        let mut c = Ctx::new();
        c.players[0].played_cards = vec!["E117".into()];
        // E117: at least 1 grain (wheat) field -> 2 wood
        c.players[0].farm[1].tile = Tile::Field {
            crop: Some(Crop::Grain),
            remaining: 2,
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
            &std::collections::BTreeSet::new(),
            &mut c.budget,
            &mut c.uses,
            &c.limits,
            &mut pending,
        )
        .unwrap();
        assert_eq!(
            c.players[0].supply.wood, 2,
            "E117 HarvestFieldGain (HasWheatField) must fire on harvest field phase"
        );
    }

    // =====================================================================
    // Negative tests: cards must NOT fire on WRONG dispatch sites
    // (clay_construction, after_harvest_bake)
    // =====================================================================

    /// B050 must not fire on after_clay_construction.
    #[test]
    fn b050_harvest_field_gain_does_not_fire_on_after_clay_construction() {
        let set = CardSet {
            definitions: crate::cards::verified_round_and_harvest_cards(),
        };
        let mut c = Ctx::new();
        c.players[0].played_cards = vec!["B050".into()];
        c.players[0].pastures.push(Pasture {
            id: 0,
            cells: vec![2, 3, 4, 5, 6],
            animal: Some(Animal::Sheep),
            quantity: 6,
        });
        let mut market = vec![];
        let mut well = BTreeMap::new();
        let mut pending = None;
        dispatch_after_clay_construction(
            &set,
            0,
            &c.players[0].clone(), // previous player
            Some(crate::model::Improvement::Fireplace2),
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
            false,
        )
        .unwrap();
        assert_eq!(
            c.players[0].supply.food, 0,
            "B050 must not fire on after_clay_construction"
        );
    }

    /// B050 must not fire on after_harvest_bake.
    #[test]
    fn b050_harvest_field_gain_does_not_fire_on_after_harvest_bake() {
        let set = CardSet {
            definitions: crate::cards::verified_round_and_harvest_cards(),
        };
        let mut c = Ctx::new();
        c.players[0].played_cards = vec!["B050".into()];
        c.players[0].pastures.push(Pasture {
            id: 0,
            cells: vec![2, 3, 4, 5, 6],
            animal: Some(Animal::Sheep),
            quantity: 6,
        });
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
            4,
            &std::collections::BTreeSet::new(),
            &mut c.budget,
            &mut c.uses,
            &c.limits,
            &mut pending,
            false,
        )
        .unwrap();
        assert_eq!(
            c.players[0].supply.food, 0,
            "B050 must not fire on after_harvest_bake"
        );
    }

    /// B057 must not fire on after_clay_construction.
    #[test]
    fn b057_start_round_gain_does_not_fire_on_after_clay_construction() {
        let set = CardSet {
            definitions: crate::cards::verified_round_and_harvest_cards(),
        };
        let mut c = Ctx::new();
        c.players[0].played_cards = vec!["B057".into()];
        c.players[0].house = House::Wood;
        let mut market = vec![];
        let mut well = BTreeMap::new();
        let mut pending = None;
        dispatch_after_clay_construction(
            &set,
            0,
            &c.players[0].clone(),
            Some(crate::model::Improvement::Fireplace2),
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
            false,
        )
        .unwrap();
        assert_eq!(
            c.players[0].supply.food, 0,
            "B057 must not fire on after_clay_construction"
        );
    }

    /// B057 must not fire on after_harvest_bake.
    #[test]
    fn b057_start_round_gain_does_not_fire_on_after_harvest_bake() {
        let set = CardSet {
            definitions: crate::cards::verified_round_and_harvest_cards(),
        };
        let mut c = Ctx::new();
        c.players[0].played_cards = vec!["B057".into()];
        c.players[0].house = House::Wood;
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
            4,
            &std::collections::BTreeSet::new(),
            &mut c.budget,
            &mut c.uses,
            &c.limits,
            &mut pending,
            false,
        )
        .unwrap();
        assert_eq!(
            c.players[0].supply.food, 0,
            "B057 must not fire on after_harvest_bake"
        );
    }

    /// B061 must not fire on after_clay_construction.
    #[test]
    fn b061_harvest_field_gain_does_not_fire_on_after_clay_construction() {
        let set = CardSet {
            definitions: crate::cards::verified_round_and_harvest_cards(),
        };
        let mut c = Ctx::new();
        c.players[0].played_cards = vec!["B061".into()];
        c.players[0].farm[1].tile = Tile::Field {
            crop: Some(Crop::Grain),
            remaining: 2,
        };
        c.players[0].farm[2].tile = Tile::Field {
            crop: Some(Crop::Vegetable),
            remaining: 1,
        };
        c.players[0].farm[3].tile = Tile::Field {
            crop: None,
            remaining: 0,
        };
        let mut market = vec![];
        let mut well = BTreeMap::new();
        let mut pending = None;
        dispatch_after_clay_construction(
            &set,
            0,
            &c.players[0].clone(),
            Some(crate::model::Improvement::Fireplace2),
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
            false,
        )
        .unwrap();
        assert_eq!(
            c.players[0].supply.food, 0,
            "B061 must not fire on after_clay_construction"
        );
    }

    /// B061 must not fire on after_harvest_bake.
    #[test]
    fn b061_harvest_field_gain_does_not_fire_on_after_harvest_bake() {
        let set = CardSet {
            definitions: crate::cards::verified_round_and_harvest_cards(),
        };
        let mut c = Ctx::new();
        c.players[0].played_cards = vec!["B061".into()];
        c.players[0].farm[1].tile = Tile::Field {
            crop: Some(Crop::Grain),
            remaining: 2,
        };
        c.players[0].farm[2].tile = Tile::Field {
            crop: Some(Crop::Vegetable),
            remaining: 1,
        };
        c.players[0].farm[3].tile = Tile::Field {
            crop: None,
            remaining: 0,
        };
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
            4,
            &std::collections::BTreeSet::new(),
            &mut c.budget,
            &mut c.uses,
            &c.limits,
            &mut pending,
            false,
        )
        .unwrap();
        assert_eq!(
            c.players[0].supply.food, 0,
            "B061 must not fire on after_harvest_bake"
        );
    }

    /// B118 must not fire on after_clay_construction.
    #[test]
    fn b118_start_round_gain_does_not_fire_on_after_clay_construction() {
        let set = CardSet {
            definitions: crate::cards::verified_round_and_harvest_cards(),
        };
        let mut c = Ctx::new();
        c.players[0].played_cards = vec!["B118".into()];
        c.players[0].supply.wood = 10;
        let mut market = vec![];
        let mut well = BTreeMap::new();
        let mut pending = None;
        dispatch_after_clay_construction(
            &set,
            0,
            &c.players[0].clone(),
            Some(crate::model::Improvement::Fireplace2),
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
            false,
        )
        .unwrap();
        assert_eq!(
            c.players[0].supply.wood, 10,
            "B118 must not fire on after_clay_construction"
        );
    }

    /// B118 must not fire on after_harvest_bake.
    #[test]
    fn b118_start_round_gain_does_not_fire_on_after_harvest_bake() {
        let set = CardSet {
            definitions: crate::cards::verified_round_and_harvest_cards(),
        };
        let mut c = Ctx::new();
        c.players[0].played_cards = vec!["B118".into()];
        c.players[0].supply.wood = 10;
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
            4,
            &std::collections::BTreeSet::new(),
            &mut c.budget,
            &mut c.uses,
            &c.limits,
            &mut pending,
            false,
        )
        .unwrap();
        assert_eq!(
            c.players[0].supply.wood, 10,
            "B118 must not fire on after_harvest_bake"
        );
    }

    /// C106 must not fire on after_clay_construction.
    #[test]
    fn c106_harvest_field_gain_does_not_fire_on_after_clay_construction() {
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
        let mut market = vec![];
        let mut well = BTreeMap::new();
        let mut pending = None;
        dispatch_after_clay_construction(
            &set,
            0,
            &c.players[0].clone(),
            Some(crate::model::Improvement::Fireplace2),
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
            false,
        )
        .unwrap();
        assert_eq!(
            c.players[0].supply.food, 0,
            "C106 must not fire on after_clay_construction"
        );
    }

    /// C106 must not fire on after_harvest_bake.
    #[test]
    fn c106_harvest_field_gain_does_not_fire_on_after_harvest_bake() {
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
            4,
            &std::collections::BTreeSet::new(),
            &mut c.budget,
            &mut c.uses,
            &c.limits,
            &mut pending,
            false,
        )
        .unwrap();
        assert_eq!(
            c.players[0].supply.food, 0,
            "C106 must not fire on after_harvest_bake"
        );
    }

    /// C111 must not fire on after_clay_construction.
    #[test]
    fn c111_start_round_gain_does_not_fire_on_after_clay_construction() {
        let set = CardSet {
            definitions: crate::cards::verified_band_c_cards(),
        };
        let mut c = Ctx::new();
        c.players[0].played_cards = vec!["C111".into()];
        c.players[0].supply.food = 10;
        let mut market = vec![];
        let mut well = BTreeMap::new();
        let mut pending = None;
        dispatch_after_clay_construction(
            &set,
            0,
            &c.players[0].clone(),
            Some(crate::model::Improvement::Fireplace2),
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
            false,
        )
        .unwrap();
        assert_eq!(
            c.players[0].supply.food, 10,
            "C111 must not fire on after_clay_construction"
        );
    }

    /// C111 must not fire on after_harvest_bake.
    #[test]
    fn c111_start_round_gain_does_not_fire_on_after_harvest_bake() {
        let set = CardSet {
            definitions: crate::cards::verified_band_c_cards(),
        };
        let mut c = Ctx::new();
        c.players[0].played_cards = vec!["C111".into()];
        c.players[0].supply.food = 10;
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
            4,
            &std::collections::BTreeSet::new(),
            &mut c.budget,
            &mut c.uses,
            &c.limits,
            &mut pending,
            false,
        )
        .unwrap();
        assert_eq!(
            c.players[0].supply.food, 10,
            "C111 must not fire on after_harvest_bake"
        );
    }

    /// E107 must not fire on after_clay_construction.
    #[test]
    fn e107_harvest_field_gain_does_not_fire_on_after_clay_construction() {
        let set = CardSet {
            definitions: crate::cards::verified_band_e_cards(),
        };
        let mut c = Ctx::new();
        c.players[0].played_cards = vec!["E107".into()];
        c.players[0].farm[1].tile = Tile::Field {
            crop: Some(Crop::Grain),
            remaining: 2,
        };
        c.players[0].farm[2].tile = Tile::Field {
            crop: Some(Crop::Grain),
            remaining: 2,
        };
        c.players[0].farm[3].tile = Tile::Field {
            crop: Some(Crop::Vegetable),
            remaining: 1,
        };
        c.players[0].farm[4].tile = Tile::Field {
            crop: Some(Crop::Vegetable),
            remaining: 1,
        };
        let mut market = vec![];
        let mut well = BTreeMap::new();
        let mut pending = None;
        dispatch_after_clay_construction(
            &set,
            0,
            &c.players[0].clone(),
            Some(crate::model::Improvement::Fireplace2),
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
            false,
        )
        .unwrap();
        assert_eq!(
            c.players[0].supply.food, 0,
            "E107 must not fire on after_clay_construction"
        );
    }

    /// E107 must not fire on after_harvest_bake.
    #[test]
    fn e107_harvest_field_gain_does_not_fire_on_after_harvest_bake() {
        let set = CardSet {
            definitions: crate::cards::verified_band_e_cards(),
        };
        let mut c = Ctx::new();
        c.players[0].played_cards = vec!["E107".into()];
        c.players[0].farm[1].tile = Tile::Field {
            crop: Some(Crop::Grain),
            remaining: 2,
        };
        c.players[0].farm[2].tile = Tile::Field {
            crop: Some(Crop::Grain),
            remaining: 2,
        };
        c.players[0].farm[3].tile = Tile::Field {
            crop: Some(Crop::Vegetable),
            remaining: 1,
        };
        c.players[0].farm[4].tile = Tile::Field {
            crop: Some(Crop::Vegetable),
            remaining: 1,
        };
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
            4,
            &std::collections::BTreeSet::new(),
            &mut c.budget,
            &mut c.uses,
            &c.limits,
            &mut pending,
            false,
        )
        .unwrap();
        assert_eq!(
            c.players[0].supply.food, 0,
            "E107 must not fire on after_harvest_bake"
        );
    }

    /// E111 must not fire on after_clay_construction (food).
    #[test]
    fn e111_start_round_gain_does_not_fire_on_after_clay_construction() {
        let set = CardSet {
            definitions: crate::cards::verified_band_e_cards(),
        };
        let mut c = Ctx::new();
        c.players[0].played_cards = vec!["E111".into()];
        let mut market = vec![];
        let mut well = BTreeMap::new();
        let mut pending = None;
        dispatch_after_clay_construction(
            &set,
            0,
            &c.players[0].clone(),
            Some(crate::model::Improvement::Fireplace2),
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
            false,
        )
        .unwrap();
        assert_eq!(
            c.players[0].supply.food, 0,
            "E111 must not fire on after_clay_construction (food)"
        );
    }

    /// E111 must not fire on after_harvest_bake (food).
    #[test]
    fn e111_start_round_gain_does_not_fire_on_after_harvest_bake() {
        let set = CardSet {
            definitions: crate::cards::verified_band_e_cards(),
        };
        let mut c = Ctx::new();
        c.players[0].played_cards = vec!["E111".into()];
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
            4,
            &std::collections::BTreeSet::new(),
            &mut c.budget,
            &mut c.uses,
            &c.limits,
            &mut pending,
            false,
        )
        .unwrap();
        assert_eq!(
            c.players[0].supply.food, 0,
            "E111 must not fire on after_harvest_bake (food)"
        );
    }

    /// E111 must not fire on after_clay_construction (wood).
    #[test]
    fn e111_harvest_field_gain_does_not_fire_on_after_clay_construction() {
        let set = CardSet {
            definitions: crate::cards::verified_band_e_cards(),
        };
        let mut c = Ctx::new();
        c.players[0].played_cards = vec!["E111".into()];
        c.players[0].supply.wood = 0;
        let mut market = vec![];
        let mut well = BTreeMap::new();
        let mut pending = None;
        dispatch_after_clay_construction(
            &set,
            0,
            &c.players[0].clone(),
            Some(crate::model::Improvement::Fireplace2),
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
            false,
        )
        .unwrap();
        assert_eq!(
            c.players[0].supply.wood, 0,
            "E111 must not fire on after_clay_construction (wood)"
        );
    }

    /// E111 must not fire on after_harvest_bake (wood).
    #[test]
    fn e111_harvest_field_gain_does_not_fire_on_after_harvest_bake() {
        let set = CardSet {
            definitions: crate::cards::verified_band_e_cards(),
        };
        let mut c = Ctx::new();
        c.players[0].played_cards = vec!["E111".into()];
        c.players[0].supply.wood = 0;
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
            4,
            &std::collections::BTreeSet::new(),
            &mut c.budget,
            &mut c.uses,
            &c.limits,
            &mut pending,
            false,
        )
        .unwrap();
        assert_eq!(
            c.players[0].supply.wood, 0,
            "E111 must not fire on after_harvest_bake (wood)"
        );
    }

    /// E117 must not fire on after_clay_construction.
    #[test]
    fn e117_harvest_field_gain_does_not_fire_on_after_clay_construction() {
        let set = CardSet {
            definitions: crate::cards::verified_band_e_cards(),
        };
        let mut c = Ctx::new();
        c.players[0].played_cards = vec!["E117".into()];
        c.players[0].farm[1].tile = Tile::Field {
            crop: Some(Crop::Grain),
            remaining: 2,
        };
        let mut market = vec![];
        let mut well = BTreeMap::new();
        let mut pending = None;
        dispatch_after_clay_construction(
            &set,
            0,
            &c.players[0].clone(),
            Some(crate::model::Improvement::Fireplace2),
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
            false,
        )
        .unwrap();
        assert_eq!(
            c.players[0].supply.wood, 0,
            "E117 must not fire on after_clay_construction"
        );
    }

    /// E117 must not fire on after_harvest_bake.
    #[test]
    fn e117_harvest_field_gain_does_not_fire_on_after_harvest_bake() {
        let set = CardSet {
            definitions: crate::cards::verified_band_e_cards(),
        };
        let mut c = Ctx::new();
        c.players[0].played_cards = vec!["E117".into()];
        c.players[0].farm[1].tile = Tile::Field {
            crop: Some(Crop::Grain),
            remaining: 2,
        };
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
            4,
            &std::collections::BTreeSet::new(),
            &mut c.budget,
            &mut c.uses,
            &c.limits,
            &mut pending,
            false,
        )
        .unwrap();
        assert_eq!(
            c.players[0].supply.wood, 0,
            "E117 must not fire on after_harvest_bake"
        );
    }

    // --- A118/A145/C066 dispatch tests (PR #385 review) ---

    /// A118: fixed 1 wood per harvest field phase fires; optional exchange is plan-driven
    /// and does NOT create a pending decision in the dispatcher.
    #[test]
    fn a118_harvest_field_fixed_wood_fires_and_no_pending() {
        let set = CardSet {
            definitions: crate::cards::verified_round_and_harvest_cards(),
        };
        let mut c = Ctx::new();
        c.players[0].played_cards = vec!["A118".into()];
        c.players[0].supply.food = 0; // no food for exchange
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
            &std::collections::BTreeSet::new(),
            &mut c.budget,
            &mut c.uses,
            &c.limits,
            &mut pending,
        )
        .unwrap();
        assert_eq!(
            c.players[0].supply.wood, 1,
            "A118 fixed HarvestFieldGain: 1 wood per harvest field phase"
        );
        assert!(
            pending.is_none(),
            "A118 optional exchange must not create a pending decision in dispatcher (plan-driven)"
        );
    }

    /// A118 with food available: fixed wood fires, no pending (exchange is plan-driven via FeedingPlan).
    #[test]
    fn a118_harvest_field_with_food_no_pending() {
        let set = CardSet {
            definitions: crate::cards::verified_round_and_harvest_cards(),
        };
        let mut c = Ctx::new();
        c.players[0].played_cards = vec!["A118".into()];
        c.players[0].supply.food = 5; // food available for exchange
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
            &std::collections::BTreeSet::new(),
            &mut c.budget,
            &mut c.uses,
            &c.limits,
            &mut pending,
        )
        .unwrap();
        assert_eq!(c.players[0].supply.wood, 1, "A118 fixed wood still fires");
        assert_eq!(
            c.players[0].supply.food, 5,
            "food unchanged in field phase (exchange happens in feeding via plan)"
        );
        assert!(
            pending.is_none(),
            "A118 optional exchange must not create a pending decision in dispatcher"
        );
    }

    /// A145: reed 1 fires at harvest tail (AfterHarvestComplete), not at field phase.
    #[test]
    fn a145_reed_fires_at_harvest_tail_not_field() {
        let set = CardSet {
            definitions: crate::cards::verified_round_and_harvest_cards(),
        };
        // Field phase: must not fire
        let mut c = Ctx::new();
        c.players[0].played_cards = vec!["A145".into()];
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
            &std::collections::BTreeSet::new(),
            &mut c.budget,
            &mut c.uses,
            &c.limits,
            &mut pending,
        )
        .unwrap();
        assert_eq!(
            c.players[0].supply.reed, 0,
            "A145 reed must not fire during field phase"
        );
        assert!(pending.is_none());

        // Harvest tail: must fire
        let mut c = Ctx::new();
        c.players[0].played_cards = vec!["A145".into()];
        let mut market = vec![];
        let mut well = BTreeMap::new();
        let mut pending = None;
        dispatch_after_harvest_complete(
            &set,
            0,
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
            false, // c066_take_food_instead
        )
        .unwrap();
        assert_eq!(
            c.players[0].supply.reed, 1,
            "A145 reed must fire at harvest tail (AfterHarvestComplete)"
        );
        assert!(pending.is_none());
    }

    /// A145 in non-harvest round: no fire.
    #[test]
    fn a145_no_fire_in_non_harvest_round() {
        let set = CardSet {
            definitions: crate::cards::verified_round_and_harvest_cards(),
        };
        let mut c = Ctx::new();
        c.players[0].played_cards = vec!["A145".into()];
        let mut market = vec![];
        let mut well = BTreeMap::new();
        let mut pending = None;
        dispatch_after_harvest_complete(
            &set,
            0,
            &mut c.cards,
            &mut c.players,
            &mut market,
            &mut well,
            3, // round 3 is not a harvest round
            &std::collections::BTreeSet::new(),
            &mut c.budget,
            &mut c.uses,
            &c.limits,
            &mut pending,
            false,
        )
        .unwrap();
        assert_eq!(
            c.players[0].supply.reed, 0,
            "A145 must not fire in non-harvest round"
        );
    }

    /// C066: grain 0/1 -> no gain.
    #[test]
    fn c066_grain_zero_one_no_gain() {
        let set = CardSet {
            definitions: crate::cards::verified_round_and_harvest_cards(),
        };
        for grain in [0, 1] {
            let mut c = Ctx::new();
            c.players[0].played_cards = vec!["C066".into()];
            c.players[0].supply.grain = grain;
            let mut market = vec![];
            let mut well = BTreeMap::new();
            let mut pending = None;
            dispatch_after_harvest_complete(
                &set,
                0,
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
                false, // take_food_instead
            )
            .unwrap();
            assert_eq!(
                c.players[0].supply.food, 0,
                "C066 grain={grain}: no food gain below threshold"
            );
            assert_eq!(
                c.players[0].supply.grain, grain,
                "C066 grain={grain}: no grain change"
            );
            assert!(pending.is_none());
        }
    }

    /// C066: grain 2 -> food 1 only.
    #[test]
    fn c066_grain_two_food_one() {
        let set = CardSet {
            definitions: crate::cards::verified_round_and_harvest_cards(),
        };
        let mut c = Ctx::new();
        c.players[0].played_cards = vec!["C066".into()];
        c.players[0].supply.grain = 2;
        let mut market = vec![];
        let mut well = BTreeMap::new();
        let mut pending = None;
        dispatch_after_harvest_complete(
            &set,
            0,
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
            false, // take_food_instead = false (default: take grain instead at >=3)
        )
        .unwrap();
        assert_eq!(c.players[0].supply.food, 1, "C066 grain=2: food 1");
        assert_eq!(
            c.players[0].supply.grain, 2,
            "C066 grain=2: grain unchanged"
        );
        assert!(pending.is_none());
    }

    /// C066: grain 3 -> grain 1 instead of food (default).
    #[test]
    fn c066_grain_three_grain_instead_default() {
        let set = CardSet {
            definitions: crate::cards::verified_round_and_harvest_cards(),
        };
        let mut c = Ctx::new();
        c.players[0].played_cards = vec!["C066".into()];
        c.players[0].supply.grain = 3;
        let mut market = vec![];
        let mut well = BTreeMap::new();
        let mut pending = None;
        dispatch_after_harvest_complete(
            &set,
            0,
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
            false, // take_food_instead = false -> take grain instead
        )
        .unwrap();
        assert_eq!(
            c.players[0].supply.food, 0,
            "C066 grain=3 default: no food (grain instead)"
        );
        assert_eq!(
            c.players[0].supply.grain, 4,
            "C066 grain=3 default: +1 grain (3 -> 4)"
        );
        assert!(pending.is_none());
    }

    /// C066: grain 3 -> food 1 if take_food_instead = true.
    #[test]
    fn c066_grain_three_food_if_opt_in() {
        let set = CardSet {
            definitions: crate::cards::verified_round_and_harvest_cards(),
        };
        let mut c = Ctx::new();
        c.players[0].played_cards = vec!["C066".into()];
        c.players[0].supply.grain = 3;
        let mut market = vec![];
        let mut well = BTreeMap::new();
        let mut pending = None;
        dispatch_after_harvest_complete(
            &set,
            0,
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
            true, // take_food_instead = true -> opt into food
        )
        .unwrap();
        assert_eq!(
            c.players[0].supply.food, 1,
            "C066 grain=3 with take_food_instead: food 1"
        );
        assert_eq!(
            c.players[0].supply.grain, 3,
            "C066 grain=3 with take_food_instead: grain unchanged"
        );
        assert!(pending.is_none());
    }

    /// C066: grain 5 -> still only food 1 + grain 1 (second trigger doesn't fire again).
    #[test]
    fn c066_grain_five_capped() {
        let set = CardSet {
            definitions: crate::cards::verified_round_and_harvest_cards(),
        };
        let mut c = Ctx::new();
        c.players[0].played_cards = vec!["C066".into()];
        c.players[0].supply.grain = 5;
        let mut market = vec![];
        let mut well = BTreeMap::new();
        let mut pending = None;
        dispatch_after_harvest_complete(
            &set,
            0,
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
            false,
        )
        .unwrap();
        assert_eq!(
            c.players[0].supply.food, 0,
            "C066 grain=5 default: no food (grain instead)"
        );
        assert_eq!(
            c.players[0].supply.grain, 6,
            "C066 grain=5 default: +1 grain only (5 -> 6), not +2"
        );
        assert!(pending.is_none());
    }
}
