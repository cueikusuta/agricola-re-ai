#!/usr/bin/env python3
"""Fix all remaining issues in card_events.rs and engine.rs"""

# Fix card_events.rs
with open('src/card_events.rs', 'r') as f:
    content = f.read()

# Fix 1: dispatch_harvest_field got merged incorrectly - need to separate it
# The issue is that the patch merged the end of dispatch_after_clay_construction with dispatch_harvest_field
# Let me find and fix this

# Find the problematic section
bad_pattern = '''        Ok(Transition::Completed)
    }

    /// Dispatch HarvestField phase during harvest.
#[allow(clippy::too_many_arguments)] // threads the same storages as the legacy cards.rs fns
pub fn dispatch_harvest_field('''

good_pattern = '''        Ok(Transition::Completed)
}

/// Dispatch HarvestField phase during harvest.
#[allow(clippy::too_many_arguments)] // threads the same storages as the legacy cards.rs fns
pub fn dispatch_harvest_field('''

content = content.replace(bad_pattern, good_pattern)

# Fix 2: dispatch_harvest_field needs the allow_choose_trigger_order_pause parameter
old_harvest_sig = '''/// Dispatch HarvestField phase during harvest.
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
) -> Result<Transition, String> {'''

new_harvest_sig = '''/// Dispatch HarvestField phase during harvest.
/// If `allow_choose_trigger_order_pause` is true, enables ChooseTriggerOrder pause
/// for post-work chain integration (Issue #270).
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
    allow_choose_trigger_order_pause: bool,
) -> Result<Transition, String> {'''

content = content.replace(old_harvest_sig, new_harvest_sig)

# Fix 3: dispatch_harvest_field resolve_phase calls need to use the parameter
old_harvest_calls = '''    resolve_phase(
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

new_harvest_calls = '''    resolve_phase(
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

content = content.replace(old_harvest_calls, new_harvest_calls)

# Fix 4: dispatch_after_harvest_bake needs the parameter
old_bake_sig = '''/// Dispatch AfterHarvestBake phase when baking occurs.
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
) -> Result<Transition, String> {'''

new_bake_sig = '''/// Dispatch AfterHarvestBake phase when baking occurs.
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
) -> Result<Transition, String> {'''

content = content.replace(old_bake_sig, new_bake_sig)

# Fix 5: dispatch_after_harvest_bake resolve_phase calls
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
    resolve_phase(
        &mut ctx,
        player,
        TriggerPhase::Interrupt,
        &trigger,
        &all_players,
        true, // post-work chain: allow ChooseTriggerOrder pause
    )'''

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
    resolve_phase(
        &mut ctx,
        player,
        TriggerPhase::Interrupt,
        &trigger,
        &all_players,
        allow_choose_trigger_order_pause,
    )'''

content = content.replace(old_bake_calls, new_bake_calls)

# Fix 6: dispatch_start_round doesn't need the parameter (not post-work chain), but needs to pass false to resolve_phase
# The current code already passes false, but let's make sure it doesn't have the parameter
# Actually, start_round should NOT have the parameter since it's not post-work chain

# Write back
with open('src/card_events.rs', 'w') as f:
    f.write(content)

print("card_events.rs fixed!")

# Now fix engine.rs
with open('src/engine.rs', 'r') as f:
    engine_content = f.read()

# Update dispatch_after_worker call in apply_post_work_card_effects
old_worker_call = '''        let result = crate::card_events::dispatch_after_worker(
            &self.card_set,
            player,
            action.space,
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
        )?;'''

new_worker_call = '''        let result = crate::card_events::dispatch_after_worker(
            &self.card_set,
            player,
            action.space,
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
            true, // post-work chain: allow ChooseTriggerOrder pause
        )?;'''

engine_content = engine_content.replace(old_worker_call, new_worker_call)

# Update dispatch_after_clay_construction call
old_clay_call = '''        let result = crate::card_events::dispatch_after_clay_construction(
            &self.card_set,
            player,
            previous,
            bought,
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
        )?;'''

new_clay_call = '''        let result = crate::card_events::dispatch_after_clay_construction(
            &self.card_set,
            player,
            previous,
            bought,
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
            true, // post-work chain: allow ChooseTriggerOrder pause
        )?;'''

engine_content = engine_content.replace(old_clay_call, new_clay_call)

# Update dispatch_after_harvest_bake call
old_bake_call = '''            let result = crate::card_events::dispatch_after_harvest_bake(
                &self.card_set,
                player,
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
            )?;'''

new_bake_call = '''            let result = crate::card_events::dispatch_after_harvest_bake(
                &self.card_set,
                player,
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
                true, // post-work chain: allow ChooseTriggerOrder pause
            )?;'''

engine_content = engine_content.replace(old_bake_call, new_bake_call)

with open('src/engine.rs', 'w') as f:
    f.write(engine_content)

print("engine.rs fixed!")