with open('src/card_events.rs', 'r') as f:
    content = f.read()

# Check effect_phase function more carefully
idx = content.find('fn effect_phase')
if idx >= 0:
    section = content[idx:idx+1000]
    print('=== effect_phase ===')
    print(section)
    print()
    print('Has AfterImprovementPlayedGain:', 'AfterImprovementPlayedGain' in section)
    print('Has AfterOccupationGain:', 'AfterOccupationGain' in section)

# Check resolve_phase_scan
idx = content.find('fn resolve_phase_scan')
if idx >= 0:
    section = content[idx:idx+2000]
    print('\n=== resolve_phase_scan (first 2000 chars) ===')
    # Find the match arms area
    match_idx = section.find('CardEffect::AfterOccupationGainPerFoodCost')
    if match_idx >= 0:
        print(section[match_idx:match_idx+500])
    print()
    print('Has AfterImprovementPlayedGain:', 'AfterImprovementPlayedGain' in section)
    print('Has AfterOccupationGain:', 'AfterOccupationGain' in section)