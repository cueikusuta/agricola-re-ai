with open('src/card_events.rs', 'r') as f:
    content = f.read()

# Check resolve_phase_scan function
idx = content.find('fn resolve_phase_scan')
if idx >= 0:
    brace_count = 0
    in_function = False
    for i in range(idx, len(content)):
        if content[i] == '{':
            brace_count += 1
            in_function = True
        elif content[i] == '}':
            brace_count -= 1
            if in_function and brace_count == 0:
                section = content[idx:i+1]
                print('=== resolve_phase_scan ===')
                print('Has AfterImprovementPlayedGain:', 'AfterImprovementPlayedGain' in section)
                print('Has AfterOccupationGain:', 'AfterOccupationGain' in section)
                # Show the match arms area
                match_idx = section.find('CardEffect::AfterOccupationGainPerFoodCost')
                if match_idx >= 0:
                    print(section[match_idx:match_idx+800])
                break

# Check effect_condition_holds
idx = content.find('fn effect_condition_holds')
if idx >= 0:
    brace_count = 0
    in_function = False
    for i in range(idx, len(content)):
        if content[i] == '{':
            brace_count += 1
            in_function = True
        elif content[i] == '}':
            brace_count -= 1
            if in_function and brace_count == 0:
                section = content[idx:i+1]
                print('\n=== effect_condition_holds ===')
                print('Has AfterImprovementPlayedGain:', 'AfterImprovementPlayedGain' in section)
                print('Has AfterOccupationGain:', 'AfterOccupationGain' in section)
                break

# Check execute_chosen_triggers
idx = content.find('fn execute_chosen_triggers')
if idx >= 0:
    brace_count = 0
    in_function = False
    for i in range(idx, len(content)):
        if content[i] == '{':
            brace_count += 1
            in_function = True
        elif content[i] == '}':
            brace_count -= 1
            if in_function and brace_count == 0:
                section = content[idx:i+1]
                print('\n=== execute_chosen_triggers ===')
                print('Has AfterImprovementPlayedGain:', 'AfterImprovementPlayedGain' in section)
                print('Has AfterOccupationGain:', 'AfterOccupationGain' in section)
                break

# Check dispatch_play for Phase 4b
idx = content.find('fn dispatch_play')
if idx >= 0:
    brace_count = 0
    in_function = False
    for i in range(idx, len(content)):
        if content[i] == '{':
            brace_count += 1
            in_function = True
        elif content[i] == '}':
            brace_count -= 1
            if in_function and brace_count == 0:
                section = content[idx:i+1]
                print('\n=== dispatch_play ===')
                print('Has Phase 4b:', 'Phase 4b' in section)
                print('Has AfterImprovementPlayedGain:', 'AfterImprovementPlayedGain' in section)
                break

# Check dispatch_after_worker_any_player exists
idx = content.find('fn dispatch_after_worker_any_player')
print(f'\ndispatch_after_worker_any_player: {"FOUND" if idx >= 0 else "MISSING"} at {idx}')