with open('src/card_events.rs', 'r') as f:
    content = f.read()

# Check for any remaining conflict markers
if '<<<<<<<' in content:
    print('CONFLICT MARKERS FOUND!')
    for i, line in enumerate(content.split('\n')):
        if '<<<<<<<' in line or '=======' in line or '>>>>>>>' in line:
            print(f'Line {i+1}: {line}')
else:
    print('No conflict markers in card_events.rs')

# Check the two functions
for fn_name in ['fn dispatch_after_worker_any_player', 'pub fn dispatch_after_worker']:
    idx = content.find(fn_name)
    if idx >= 0:
        print(f'\n{fn_name} found at {idx}:')
        print(content[idx:idx+200])