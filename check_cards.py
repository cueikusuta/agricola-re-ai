with open('src/cards.rs', 'r') as f:
    content = f.read()

if '<<<<<<<' in content:
    print('CONFLICT MARKERS FOUND!')
    for i, line in enumerate(content.split('\n')):
        if '<<<<<<<' in line or '=======' in line or '>>>>>>>' in line:
            print(f'Line {i+1}: {line}')
else:
    print('No conflict markers in cards.rs')