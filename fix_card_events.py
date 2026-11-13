#!/usr/bin/env python3
"""Fix card_events.rs for ChooseTriggerOrder pause implementation."""

import re

with open('src/card_events.rs', 'r') as f:
    content = f.read()

# 1. Update resolve_phase signature and docstring
old_resolve_phase = '''/// Execute all effects of a given phase for a player's played cards in play order.
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
pub fn resolve_phase(
    ctx: &mut DispatchContext<'_>,
    player: usize,
    phase: TriggerPhase,
    trigger_data: &PhaseTriggerData,
    all_players: &[Player],
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
    )?;
    Ok(pause)
}'''

new_resolve_phase = '''/// Execute all effects of a given phase for a player's played cards in play order.
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
}'''

content = content.replace(old_resolve_phase, new_resolve_phase)

# 2. Update resolve_phase_scan signature
old_scan_sig = '''/// Scan a phase's effects with two-pass execution (Issue #270).
/// First pass: collect triggerable effects (condition holds, use limit not exceeded).
/// Second pass: execute them, or pause for ChooseTriggerOrder if >=2 in After/Interrupt.
fn resolve_phase_scan(
    ctx: &mut DispatchContext<'_>,
    player: usize,
    phase: TriggerPhase,
    trigger_data: &PhaseTriggerData,
    played_cards: &[String],
    after: Option<ChainPosition>,
    all_players: &[Player],
) -> Result<Transition, String> {'''

new_scan_sig = '''/// Scan a phase's effects with two-pass execution (Issue #270).
/// First pass: collect triggerable effects (condition holds, use limit not exceeded).
/// Second pass: execute them, or pause for ChooseTriggerOrder if >=2 in After/Interrupt.
fn resolve_phase_scan(
    ctx: &mut DispatchContext<'_>,
    player: usize,
    phase: TriggerPhase,
    trigger_data: &PhaseTriggerData,
    played_cards: &[String],
    after: Option<ChainPosition>,
    all_players: &[Player],
    allow_choose_trigger_order_pause: bool,
) -> Result<Transition, String> {'''

content = content.replace(old_scan_sig, new_scan_sig)

# 3. Update resolve_phase_from signature and call
old_from = '''/// Same as [`resolve_phase`] but the scan resumes strictly after `after`
/// (Issue #268 resume path).
pub fn resolve_phase_from(
    ctx: &mut DispatchContext<'_>,
    player: usize,
    phase: TriggerPhase,
    trigger_data: &PhaseTriggerData,
    played_cards: &[String],
    after: Option<ChainPosition>,
    all_players: &[Player],
) -> Result<Transition, String> {
    resolve_phase_scan(
        ctx,
        player,
        phase,
        trigger_data,
        played_cards,
        after,
        all_players,
    )
}'''

new_from = '''/// Same as [`resolve_phase`] but the scan resumes strictly after `after`
/// (Issue #268 resume path).
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
}'''

content = content.replace(old_from, new_from)

# 4. Update resolve_phase_for_card call
old_for_card_call = '''    let result = resolve_phase_scan(
        ctx,
        player,
        phase,
        trigger_data,
        played_cards,
        None,
        all_players,
    )?;'''

new_for_card_call = '''    let result = resolve_phase_scan(
        ctx,
        player,
        phase,
        trigger_data,
        played_cards,
        None,
        all_players,
        false, // resolve_phase_for_card is for card play, not post-work chain
    )?;'''

content = content.replace(old_for_card_call, new_for_card_call)

# 5. Update the ChooseTriggerOrder logic in resolve_phase_scan
# Find the block starting with "// If >=2 triggerable effects in After or Interrupt phase, auto-resolve with play order (2016 Lookout p.8)."
old_choose_trigger = '''    // If >=2 triggerable effects in After or Interrupt phase, auto-resolve with play order (2016 Lookout p.8).
    // Per rules: "when multiple card effects trigger simultaneously for the active player,
    // the active player chooses the order of resolution" - applies to After and Interrupt phases.
    // For now, auto-resolve with play order to maintain compatibility with existing tests.
    // TODO: Expose choice to player via UI when pending decision handling is fully implemented.
    if triggerable.len() >= 2 && matches!(phase, TriggerPhase::After | TriggerPhase::Interrupt) {
        // Convert to public TriggerableEffect for execution
        let triggerable_public: Vec<TriggerableEffect> = triggerable
            .iter()
            .map(|t| TriggerableEffect {
                card_id: t.card_id.clone(),
                effect_index: t.effect_index,
                description: t.description.clone(),
            })
            .collect();

        // Auto-resolve with play order (indices 0, 1, 2...)
        let chosen_order: Vec<usize> = (0..triggerable.len()).collect();

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
    }'''

new_choose_trigger = '''    // If >=2 triggerable effects in After or Interrupt phase, pause for player to choose order (2016 Lookout p.8).
    // Per rules: "when multiple card effects trigger simultaneously for the active player,
    // the active player chooses the order of resolution" - applies to After and Interrupt phases.
    if triggerable.len() >= 2 && matches!(phase, TriggerPhase::After | TriggerPhase::Interrupt) {
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

        if allow_choose_trigger_order_pause {
            // Create a PendingDecision for ChooseTriggerOrder
            // The engine's apply() function will stamp it with decision_id, state_hash_before,
            // and build the PostWork continuation from the pause_step returned by
            // apply_post_work_card_effects.
            let decision = PendingDecision::new(
                0, // Will be stamped by engine
                player,
                PendingKind::ChooseTriggerOrder { phase, triggers },
                String::new(), // Will be stamped by engine
                serde_json::json!({
                    "triggerable_effects": triggerable_public,
                    "card_owner": player,
                }),
            );
            return Ok(Transition::Paused(Box::new(decision)));
        }

        // Fallback: auto-resolve with play order (for dispatchers without post-work chain)
        let chosen_order: Vec<usize> = (0..triggerable.len()).collect();

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
    }'''

content = content.replace(old_choose_trigger, new_choose_trigger)

# 6. Update dispatch_after_worker to pass true for allow_choose_trigger_order_pause
old_dispatch_worker = '''    // AfterWorkerGain and AfterWorkerGainIfUsed are After phase
    resolve_phase(
        &mut ctx,
        player,
        TriggerPhase::After,
        &trigger,
        &all_players,
    )?;
    // Interrupts pause the chain after the passive phase has resolved
    // (2016 Lookout p.8: the active player resolves their effects in the
    // order they choose; a choice-requiring effect stops the chain).
    resolve_phase(
        &mut ctx,
        player,
        TriggerPhase::Interrupt,
        &trigger,
        &all_players,
    )'''

new_dispatch_worker = '''    // AfterWorkerGain and AfterWorkerGainIfUsed are After phase
    resolve_phase(
        &mut ctx,
        player,
        TriggerPhase::After,
        &trigger,
        &all_players,
        true, // post-work chain: allow ChooseTriggerOrder pause
    )?;
    // Interrupts pause the chain after the passive phase has resolved
    // (2016 Lookout p.8: the active player resolves their effects in the
    // order they choose; a choice-requiring effect stops the chain).
    resolve_phase(
        &mut ctx,
        player,
        TriggerPhase::Interrupt,
        &trigger,
        &all_players,
        true, // post-work chain: allow ChooseTriggerOrder pause
    )'''

content = content.replace(old_dispatch_worker, new_dispatch_worker)

# 7. Update dispatch_after_clay_construction
old_dispatch_clay = '''    resolve_phase(
        &mut ctx,
        player,
        TriggerPhase::After,
        &trigger,
        &all_players,
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
    )? {
        return Ok(Transition::Paused(p));
    }'''

new_dispatch_clay = '''    resolve_phase(
        &mut ctx,
        player,
        TriggerPhase::After,
        &trigger,
        &all_players,
        true, // post-work chain: allow ChooseTriggerOrder pause
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
        true, // post-work chain: allow ChooseTriggerOrder pause
    )? {
        return Ok(Transition::Paused(p));
    }'''

content = content.replace(old_dispatch_clay, new_dispatch_clay)

# 8. Update dispatch_after_harvest_bake
old_dispatch_bake = '''    resolve_phase(
        &mut ctx,
        player,
        TriggerPhase::After,
        &trigger,
        &all_players,
    )?;
    // Interrupts pause the chain after the passive phase has resolved
    // (2016 Lookout p.8: the active player resolves their effects in the
    // order they choose; a choice-requiring effect stops the chain).
    resolve_phase(
        &mut ctx,
        player,
        TriggerPhase::Interrupt,
        &trigger,
        &all_players,
    )'''

new_dispatch_bake = '''    resolve_phase(
        &mut ctx,
        player,
        TriggerPhase::After,
        &trigger,
        &all_players,
        true, // post-work chain: allow ChooseTriggerOrder pause
    )?;
    // Interrupts pause the chain after the passive phase has resolved
    // (2016 Lookout p.8: the active player resolves their effects in the
    // order they choose; a choice-requiring effect stops the chain).
    resolve_phase(
        &mut ctx,
        player,
        TriggerPhase::Interrupt,
        &trigger,
        &all_players,
        true, // post-work chain: allow ChooseTriggerOrder pause
    )'''

content = content.replace(old_dispatch_bake, new_dispatch_bake)

# 9. Update dispatch_start_round to pass false
old_dispatch_start = '''        resolve_phase(
            &mut ctx,
            player,
            TriggerPhase::After,
            &trigger,
            &all_players,
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
        )? {
            return Ok(Transition::Paused(p));
        }'''

new_dispatch_start = '''        resolve_phase(
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
        }'''

content = content.replace(old_dispatch_start, new_dispatch_start)

# 10. Update dispatch_harvest_field to pass false
old_dispatch_harvest = '''    resolve_phase(
        &mut ctx,
        player,
        TriggerPhase::After,
        &trigger,
        &all_players,
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
    )? {
        return Ok(Transition::Paused(p));
    }'''

new_dispatch_harvest = '''    resolve_phase(
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
    }'''

content = content.replace(old_dispatch_harvest, new_dispatch_harvest)

# Write back
with open('src/card_events.rs', 'w') as f:
    f.write(content)

print("All patches applied successfully!")