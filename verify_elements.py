with open('src/card_events.rs', 'r') as f:
    content = f.read()

# Verify key elements
for elem in ['AfterImprovementPlayedGain', 'AfterOccupationGain', 'dispatch_after_worker_any_player', 'Phase 4b']:
    if elem in content:
        print(f'  {elem}: FOUND')
    else:
        print(f'  {elem}: MISSING!')

# Also check the structure
import re
# Check effect_phase has both
phase_section = content[content.find('fn effect_phase'):content.find('fn effect_phase')+500]
print('\neffect_phase has AfterImprovementPlayedGain:', 'AfterImprovementPlayedGain' in phase_section)
print('effect_phase has AfterOccupationGain:', 'AfterOccupationGain' in phase_section)

# Check resolve_phase_scan
scan_section = content[content.find('resolve_phase_scan'):content.find('resolve_phase_scan')+500]
print('\nresolve_phase_scan has AfterImprovementPlayedGain:', 'AfterImprovementPlayedGain' in scan_section)
print('resolve_phase_scan has AfterOccupationGain:', 'AfterOccupationGain' in scan_section)