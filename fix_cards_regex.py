with open('src/cards.rs', 'r') as f:
    content = f.read()

# The conflict blocks need to be resolved by keeping BOTH sets of variants
# Let me do this by finding and replacing each conflict block

# Block 1: CardEffect enum (lines 138-200)
# Pattern: <<<<<<< HEAD ... ======= ... >>>>>>> origin/main
# We want to keep everything from both sides

import re

# Fix block 1: CardEffect enum
# The pattern spans from <<<<<<< HEAD to >>>>>>> origin/main
block1_pattern = r'<<<<<<< HEAD\n(.*?)\n=======\n(.*?)\n>>>>>>> origin/main'
matches = list(re.finditer(block1_pattern, content, re.DOTALL))
print(f"Found {len(matches)} conflict blocks")

for i, m in enumerate(matches):
    print(f"\nBlock {i+1}:")
    print(f"  Start: {m.start()}, End: {m.end()}")
    print(f"  HEAD side ({len(m.group(1))} chars):")
    print(f"    {repr(m.group(1)[:200])}")
    print(f"  MAIN side ({len(m.group(2))} chars):")
    print(f"    {repr(m.group(2)[:200])}")
    
    # Combine both sides - but need to be careful about duplicates
    # For the enum, we want all variants from both sides
    combined = m.group(1) + '\n' + m.group(2)
    # But the main side might already have the HEAD variants or not
    # Let's just replace the whole conflict with the combined content
    content = content[:m.start()] + combined + content[m.end():]
    print(f"  Replaced block {i+1}")

# Check for remaining conflicts
if '<<<<<<<' in content:
    print("\nWARNING: Conflict markers still remain!")
    for i, line in enumerate(content.split('\n')):
        if '<<<<<<<' in line or '=======' in line or '>>>>>>>' in line:
            print(f'Line {i+1}: {line}')
else:
    print("\nAll conflicts resolved!")

with open('src/cards.rs', 'w') as f:
    f.write(content)