with open('src/card_events.rs', 'r') as f:
    content = f.read()
if '<<<<<<<' in content:
    print('CONFLICTS REMAIN')
    for i, line in enumerate(content.split('\n')):
        if '<<<<<<<' in line or '=======' in line or '>>>>>>>' in line:
            print(f'Line {i+1}: {line}')
else:
    print('No conflict markers')