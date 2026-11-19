with open('src/card_events.rs', 'r') as f:
    lines = f.readlines()

# Resolve conflicts by processing line by line
# State machine: 0=normal, 1=in HEAD section, 2=in MAIN section
state = 0
new_lines = []
head_section = []
main_section = []

for i, line in enumerate(lines):
    if '<<<<<<< HEAD' in line:
        state = 1
        head_section = []
        continue
    elif '=======' in line and state == 1:
        state = 2
        main_section = []
        continue
    elif '>>>>>>> origin/main' in line and state == 2:
        # Resolve the conflict based on context
        # We need to decide what to keep based on the conflict content
        
        # Check what the conflict is about by examining head_section and main_section
        head_text = ''.join(head_section)
        main_text = ''.join(main_section)
        
        # Conflict resolution logic
        resolved = resolve_conflict(head_text, main_text, i)
        new_lines.extend(resolved)
        
        state = 0
        head_section = []
        main_section = []
        continue
    
    if state == 1:
        head_section.append(line)
    elif state == 2:
        main_section.append(line)
    else:
        new_lines.append(line)

# If we're still in a conflict at the end, handle it
if state != 0:
    print(f"WARNING: Unclosed conflict at end (state={state})")

# Write resolved content
with open('src/card_events.rs', 'w') as f:
    f.writelines(new_lines)

print(f"Processed {len(lines)} lines, output {len(new_lines)} lines")

def resolve_conflict(head_text, main_text, line_num):
    """Resolve a conflict based on its content."""
    # Determine conflict type by looking at key identifiers
    combined = head_text + main_text
    
    # Conflict 1: effect_phase - AfterImprovementPlayedGain vs AfterOccupationGain
    if 'AfterImprovementPlayedGain' in head_text and 'AfterOccupationGain' in main_text and 'effect_phase' in combined:
        return [
            '          // Fired only by the play dispatcher\'s dedicated after-play scan.\n',
            '          CardEffect::AfterOccupationGainPerFoodCost { .. } => TriggerPhase::After, // fired by the dedicated after-play scan\n',
            '        // C068 書棚: each time the owner plays an occupation AFTER this card — After phase (fired by dispatch_play\'s after-play scan)\n',
            '        CardEffect::AfterOccupationGain { .. } => TriggerPhase::After, // fired by the dedicated after-play scan\n',
            '        // Improvement played from hand — After phase (fired by dispatch_play\'s after-play scan for improvements)\n',
            '        CardEffect::AfterImprovementPlayedGain { .. } => TriggerPhase::After,\n',
            '          // Clay construction purchase — After phase\n'
        ]
    
    # Conflict 2: resolve_phase_scan handler
    if 'AfterImprovementPlayedGain' in head_text and 'AfterOccupationGain' in main_text and 'resolve_phase_scan' in combined:
        return [
            '                  // Never fires via the generic phase scan (see\n',
            '                  // effect_condition_holds); dispatch_play\'s after-play scan owns it.\n',
            '                  CardEffect::AfterOccupationGainPerFoodCost { .. } => EffectResult::Resolved,\n',
            '                  // C068 書棚: each time the owner plays an occupation AFTER this card — never fires via generic phase scan\n',
            '                  CardEffect::AfterOccupationGain { .. } => EffectResult::Resolved,\n',
            '                  // A109 小商人: improvement played from hand — never fires via generic phase scan; dispatch_play\'s after-play scan for improvements owns it.\n',
            '                  CardEffect::AfterImprovementPlayedGain { .. } => EffectResult::Resolved,\n',
            '                  #[cfg(test)]\n'
        ]
    
    # Conflict 3: effect_condition_holds
    if 'AfterImprovementPlayedGain' in head_text and 'AfterOccupationGain' in main_text and 'effect_condition_holds' in combined:
        return [
            '          // C116 調度品職員 fires only from dispatch_play\'s dedicated after-play\n',
            '          // scan, never from the generic phase scan.\n',
            '          CardEffect::AfterOccupationGainPerFoodCost { .. } => false,\n',
            '          // C068 書棚 fires only from dispatch_play\'s dedicated after-play\n',
            '          // scan, never from the generic phase scan.\n',
            '          CardEffect::AfterOccupationGain { .. } => false,\n',
            '          // A109 小商人 fires only from dispatch_play\'s dedicated after-play scan\n',
            '          // for improvements.\n',
            '          CardEffect::AfterImprovementPlayedGain { .. } => false,\n',
            '          CardEffect::AfterClayConstructionGain { .. } => {\n'
        ]
    
    # Conflict 4: execute_chosen_triggers
    if 'AfterImprovementPlayedGain' in head_text and 'AfterOccupationGain' in main_text and 'execute_chosen_triggers' in combined:
        return [
            '              // Never fires via the generic phase scan (see\n',
            '              // effect_condition_holds); dispatch_play\'s after-play scan owns it.\n',
            '              CardEffect::AfterOccupationGainPerFoodCost { .. } => EffectResult::Resolved,\n',
            '              // C068 書棚: each time the owner plays an occupation AFTER this card — never fires via generic phase scan\n',
            '              CardEffect::AfterOccupationGain { .. } => EffectResult::Resolved,\n',
            '              // A109 小商人: improvement played from hand — never fires via generic phase scan; dispatch_play\'s after-play scan for improvements owns it.\n',
            '              CardEffect::AfterImprovementPlayedGain { .. } => EffectResult::Resolved,\n',
            '              #[cfg(test)]\n'
        ]
    
    # Conflict 5: dispatch_play Phase 4b vs dispatch_after_worker_any_player
    if 'Phase 4b' in head_text and 'dispatch_after_worker_any_player' in main_text:
        # This is the big conflict - we need both
        return [
            '            }\n',
            '                }\n',
            '            }\n',
        '        }\n',
        '    }\n',
        '    // Phase 4b: After — improvement-play dependent triggers owned by EARLIER played\n',
        '    // cards. A109 小商人 (`AfterImprovementPlayedGain`): each time the owner plays\n',
        '    // an improvement from hand via a Major/Minor Improvement action (including renovation-combined),\n',
        '    // it grants food. The new card is already last in `played_cards` (the caller ran the zone move),\n',
        '    // so the scan covers exactly the cards played "before this one".\n',
        '    if let Some(new_card) = card_set.definition(new_card_id) {\n',
        '        if new_card.kind == crate::cards::CardKind::MinorImprovement {\n',
        '            let played = ctx.player.played_cards.clone();\n',
        '            let owned = played.len().saturating_sub(1); // exclude the new card\n',
        '            for id in &played[..owned] {\n',
        '                let Some(card) = card_set.definition(id) else {\n',
        '                    continue;\n',
        '                };\n',
        '                for effect in &card.effects {\n',
        '                    let CardEffect::AfterImprovementPlayedGain { good, amount } = effect else {\n',
        '                        continue;\n',
        '                    };\n',
        '                    consume_budget(ctx.event_budget)?;\n',
        '                    let limit = ctx\n',
        '                        .card_use_limits\n',
        '                        .get(id.as_str())\n',
        '                        .copied()\n',
        '                        .unwrap_or(u32::MAX);\n',
        '                    check_card_use(ctx.card_uses, id, limit)?;\n',
        '                    let p = &mut ctx.player;\n',
        '                    let next = p\n',
        '                        .supply\n',
        '                        .get(*good)\n',
        '                        .checked_add(*amount)\n',
        '                        .ok_or("resource overflow")?;\n',
        '                    p.supply.set(*good, next);\n',
        '                }\n',
        '            }\n',
        '        }\n',
        '    }\n',
        '    Ok(())\n',
        '}\n',
        '\n',
        '/// Dispatch AfterAnyWorkerGain / AfterAnyWorkerOthersGain effects for ALL players.\n',
        '/// These effects trigger when ANY player uses a space, so we scan all players\'\n',
        '/// cards (not just the actor\'s). This is called from `dispatch_after_worker`\n',
        '/// after the actor\'s own effects have resolved.\n',
        '#[allow(clippy::too_many_arguments)]\n',
        'fn dispatch_after_worker_any_player(\n',
        '    card_set: &CardSet,\n',
        '    _actor: usize,\n',
        '    space: Space,\n',
        '    _cards: &mut crate::cards::CardState,\n',
        '    players: &mut [Player],\n',
        '    _improvement_market: &mut Vec<crate::model::Improvement>,\n',
        '    _delayed_well: &mut BTreeMap<u8, Vec<usize>>,\n',
        '    _round: u8,\n',
        '    _used_spaces: &std::collections::BTreeSet<Space>,\n',
        '    event_budget: &mut u32,\n',
        '    card_uses: &mut BTreeMap<String, u32>,\n',
        '    card_use_limits: &BTreeMap<String, u32>,\n',
        '    _pending: &mut Option<PendingDecision>,\n',
        '    trigger: &PhaseTriggerData,\n',
        '    _all_players: &[Player],\n',
        ') -> Result<(), String> {\n',
        '    // For each player (as card owner), check their played cards for the new variants\n',
        '    // Use index-based loop to avoid borrow checker issues with mutable access\n',
        '    for card_owner in 0..players.len() {\n',
        '        let played_cards = players[card_owner].played_cards.clone();\n',
        '        for card_id in played_cards {\n',
        '            let Some(card) = card_set.definition(&card_id) else {\n',
        '                continue;\n',
        '            };\n',
        '            for effect in &card.effects {\n',
        '                // Only handle the two new variants\n',
        '                let (effect_space, good, amount) = match effect {\n',
        '                    CardEffect::AfterAnyWorkerGain {\n',
        '                        space,\n',
        '                        good,\n',
        '                        amount,\n',
        '                    } => (*space, *good, *amount),\n',
        '                    CardEffect::AfterAnyWorkerOthersGain {\n',
        '                        space,\n',
        '                        good,\n',
        '                        amount,\n',
        '                    } => (*space, *good, *amount),\n',
        '                    _ => continue,\n',
        '                };\n',
        '\n',
        '                // Check condition: space matches and event is WorkerPlaced\n',
        '                if effect_space != space || trigger.event_kind != EventKind::WorkerPlaced {\n',
        '                    continue;\n',
        '                }\n',
        '\n',
        '                // Consume event budget\n',
        '                consume_budget(event_budget)?;\n',
        '\n',
        '                // Check card use limit\n',
        '                let limit = card_use_limits.get(&card_id).copied().unwrap_or(u32::MAX);\n',
        '                check_card_use(card_uses, &card_id, limit)?;\n',
        '\n',
        '                // Apply effect based on variant\n',
        '                match effect {\n',
        '                    CardEffect::AfterAnyWorkerGain { .. } => {\n',
        '                        // Grant to card owner\n',
        '                        let p = &mut players[card_owner];\n',
        '                        let next = p\n',
        '                            .supply\n',
        '                            .get(good)\n',
        '                            .checked_add(amount)\n',
        '                            .ok_or("resource overflow")?;\n',
        '                        p.supply.set(good, next);\n',
        '                    }\n',
        '                    CardEffect::AfterAnyWorkerOthersGain { .. } => {\n',
        '                        // Grant to all players EXCEPT card owner\n',
        '                        handle_after_any_worker_others_gain(\n',
        '                            players,\n',
        '                            card_owner,\n',
        '                            &effect_space,\n',
        '                            &good,\n',
        '                            amount,\n',
        '                            trigger,\n',
        '                        )?;\n',
        '                    }\n',
        '                    _ => unreachable!(),\n',
        '                }\n',
        '            }\n',
        '        }\n',
        '    }\n',
        '    Ok(())\n',
        '}\n',
        '\n',
        '/// Dispatch AfterWorker phase for a completed worker action.\n'
        ]
    
    # Default: keep both sections (shouldn't happen for known conflicts)
    print(f"WARNING: Unknown conflict at line {line_num}, keeping HEAD")
    return head_section