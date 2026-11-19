with open('src/card_events.rs', 'r') as f:
    content = f.read()

# Find all occurrences of the variants
for variant in ['AfterImprovementPlayedGain', 'AfterOccupationGain']:
    positions = []
    idx = 0
    while True:
        idx = content.find(variant, idx)
        if idx == -1:
            break
        # Get context
        start = max(0, idx - 50)
        end = min(len(content), idx + 50)
        context = content[start:end].replace('\n', '\\n')
        positions.append((idx, context))
        idx += 1
    print(f'\n{variant} found at {len(positions)} locations:')
    for pos, ctx in positions[:5]:
        print(f'  Pos {pos}: ...{ctx}...')

# Also check the effect_phase function boundaries
idx = content.find('fn effect_phase')
if idx >= 0:
    # Find the end of the function
    brace_count = 0
    in_function = False
    for i in range(idx, len(content)):
        if content[i] == '{':
            brace_count += 1
            in_function = True
        elif content[i] == '}':
            brace_count -= 1
            if in_function and brace_count == 0:
                print(f'\neffect_phase function: {idx} to {i+1}')
                print(content[idx:i+1])
                break