with open('src/policy.rs', 'r') as f:
    content = f.read()

if '<<<<<<<' in content:
    print('CONFLICTS FOUND in policy.rs')
    for i, line in enumerate(content.split('\n')):
        if '<<<<<<<' in line or '=======' in line or '>>>>>>>' in line:
            print(f'Line {i+1}: {line}')
else:
    print('policy.rs: OK - no conflict markers')