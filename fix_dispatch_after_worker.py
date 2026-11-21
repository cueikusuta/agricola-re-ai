# Read the current file
with open('src/card_events.rs', 'r') as f:
    content = f.read()

# Find the section to replace
start_marker = "    )?;\n\n    // Recreate ctx for Interrupt phase (re-borrow the mutables)"
end_marker = "        allow_choose_trigger_order_pause,\n    )\n}\n\n/// Dispatch StartRound phase for all players at round start."

start_idx = content.find(start_marker)
end_idx = content.find(end_marker)

if start_idx == -1:
    print("Start marker not found!")
elif end_idx == -1:
    print("End marker not found!")
else:
    end_idx += len(end_marker)
    print(f"Found section from {start_idx} to {end_idx}")
    
    # Replacement content
    replacement = """    )?;

    // Recreate ctx for Interrupt phase (re-borrow the mutables)
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

/// Dispatch StartRound phase for all players at round start."""

    new_content = content[:start_idx] + replacement + content[end_idx:]
    
    with open('src/card_events.rs', 'w') as f:
        f.write(new_content)
    
    print("Replacement done!")