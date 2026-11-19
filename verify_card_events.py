with open('src/card_events.rs', 'r') as f:
    content = f.read()
if '<<<<<<<' in content:
    print('CONFLICTS REMAIN in card_events.rs')
    for i, line in enumerate(content.split('\n')):
        if '<<<<<<<' in line or '=======' in line or '>>>>>>>' in line:
            print(f'Line {i+1}: {line}')
else:
    print('card_events.rs: OK - no conflict markers')
    # Verify key elements
    for elem in ['AfterImprovementPlayedGain', 'AfterOccupationGain', 'dispatch_after_worker_any_player', 'Phase 4b']:
        if elem in content:
            print(f'  {elem}: FOUND')
        else:
            print(f'  {elem}: MISSING!')