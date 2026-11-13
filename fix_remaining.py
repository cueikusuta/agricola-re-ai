#!/usr/bin/env python3
"""Fix remaining issues"""

with open('src/card_events.rs', 'r') as f:
    content = f.read()

# Fix 1: dispatch_after_harvest_bake - use the parameter instead of hardcoded true
old_bake_calls = '''    resolve_phase(
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

new_bake_calls = '''    resolve_phase(
        &mut ctx,
        player,
        TriggerPhase::After,
        &trigger,
        &all_players,
        allow_choose_trigger_order_pause,
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
        allow_choose_trigger_order_pause,
    )? {
        return Ok(Transition::Paused(p));
    }'''

content = content.replace(old_bake_calls, new_bake_calls)

# Fix 2: Test call at line 2914 - add false parameter
old_test_call = '''        let result = dispatch_after_worker(
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
        );'''

new_test_call = '''        let result = dispatch_after_worker(
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
        );'''

content = content.replace(old_test_call, new_test_call)

# Write back
with open('src/card_events.rs', 'w') as f:
    f.write(content)

print("Remaining fixes applied!")