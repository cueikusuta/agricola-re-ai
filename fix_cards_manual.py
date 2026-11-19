with open('src/cards.rs', 'r') as f:
    lines = f.readlines()

# The file has conflict markers but no separators. 
# Based on the git diff, I need to reconstruct the correct version.
# Let me find the three conflict regions and fix them manually.

new_lines = []
i = 0
while i < len(lines):
    line = lines[i]
    
    # Conflict 1: CardEffect enum - starts at line with <<<<<<< HEAD after OnPlayGainPerCount
    if '<<<<<<< HEAD' in line and i > 130 and i < 150:
        print(f"Found conflict 1 at line {i+1}")
        # Skip the <<<<<<< HEAD line
        i += 1
        # Add AfterImprovementPlayedGain (HEAD side)
        while i < len(lines) and '=======' not in lines[i] and '>>>>>>>' not in lines[i]:
            # But we need to also include the main side variants
            # Let's just collect until we see the end of the enum
            new_lines.append(lines[i])
            i += 1
        # Now we're at either ======= or >>>>>>>
        # Skip to after >>>>>>>
        while i < len(lines) and '>>>>>>>' not in lines[i]:
            i += 1
        if i < len(lines) and '>>>>>>>' in lines[i]:
            i += 1
        continue
    
    # Conflict 2: public_tag match
    if '<<<<<<< HEAD' in line and i > 230 and i < 250:
        print(f"Found conflict 2 at line {i+1}")
        i += 1
        while i < len(lines) and '>>>>>>>' not in lines[i]:
            new_lines.append(lines[i])
            i += 1
        if i < len(lines) and '>>>>>>>' in lines[i]:
            i += 1
        continue
    
    # Conflict 3: validate function
    if '<<<<<<< HEAD' in line and i > 460 and i < 480:
        print(f"Found conflict 3 at line {i+1}")
        i += 1
        while i < len(lines) and '>>>>>>>' not in lines[i]:
            new_lines.append(lines[i])
            i += 1
        if i < len(lines) and '>>>>>>>' in lines[i]:
            i += 1
        continue
    
    new_lines.append(line)
    i += 1

# Check result
result = ''.join(new_lines)
if '<<<<<<<' in result:
    print("WARNING: Conflict markers remain!")
    for idx, l in enumerate(result.split('\n')):
        if '<<<<<<<' in l or '=======' in l or '>>>>>>>' in l:
            print(f"Line {idx+1}: {l}")
else:
    print("All conflict markers removed!")

with open('src/cards.rs', 'w') as f:
    f.write(result)

print("Done")