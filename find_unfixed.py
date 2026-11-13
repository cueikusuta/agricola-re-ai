#!/usr/bin/env python3
"""Fix all remaining test calls"""

with open('src/card_events.rs', 'r') as f:
    content = f.read()

# Fix dispatch_after_clay_construction with None improvement
old_clay_none = '''        dispatch_after_clay_construction(
            &set,
            0,
            &previous,
            None,
            &mut c.cards,
            &mut c.players,
            &mut market,
            &mut well,
            4,
            &std::collections::BTreeSet::new(),
            &mut c.budget,
            &mut c.uses,
            &c.limits,
            &mut pending,
        )'''

new_clay_none = '''        dispatch_after_clay_construction(
            &set,
            0,
            &previous,
            None,
            &mut c.cards,
            &mut c.players,
            &mut market,
            &mut well,
            4,
            &std::collections::BTreeSet::new(),
            &mut c.budget,
            &mut c.uses,
            &c.limits,
            &mut pending,
            false, // test: not post-work chain
        )'''

content = content.replace(old_clay_none, new_clay_none)

# Also check for any other dispatch_after_worker calls that might have been missed
# Look for patterns with &mut pending, ) without the bool
import re

# Find all dispatch_after_worker calls that end with &mut pending,\n        )
pattern = r'(dispatch_after_worker\([^)]*&mut pending,\n\s+\))'
matches = re.findall(pattern, content, re.MULTILINE | re.DOTALL)
for m in matches:
    if 'false' not in m and 'true' not in m:
        print(f"Found unfixed dispatch_after_worker call:\n{m[:200]}")

# Find all dispatch_after_clay_construction calls
pattern2 = r'(dispatch_after_clay_construction\([^)]*&mut pending,\n\s+\))'
matches2 = re.findall(pattern2, content, re.MULTILINE | re.DOTALL)
for m in matches2:
    if 'false' not in m and 'true' not in m:
        print(f"Found unfixed dispatch_after_clay_construction call:\n{m[:200]}")

# Find all dispatch_after_harvest_bake calls
pattern3 = r'(dispatch_after_harvest_bake\([^)]*&mut pending,\n\s+\))'
matches3 = re.findall(pattern3, content, re.MULTILINE | re.DOTALL)
for m in matches3:
    if 'false' not in m and 'true' not in m:
        print(f"Found unfixed dispatch_after_harvest_bake call:\n{m[:200]}")

with open('src/card_events.rs', 'w') as f:
    f.write(content)

print("Done!")