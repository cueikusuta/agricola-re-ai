with open('src/registry.rs', 'r') as f:
    content = f.read()

if '<<<<<<<' in content:
    print('CONFLICTS FOUND in registry.rs')
    for i, line in enumerate(content.split('\n')):
        if '<<<<<<<' in line or '=======' in line or '>>>>>>>' in line:
            print(f'Line {i+1}: {line}')
else:
    print('registry.rs: OK - no conflict markers')