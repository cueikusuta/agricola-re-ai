#!/usr/bin/env python3
"""Fix the main conflict with correct dispatch_play Ok(()) location"""

with open("src/card_events.rs", "r") as f:
    lines = f.read().split('\n')

# Known line numbers:
head_start = 1835
equals_idx = 1868
origin_end = 1954

# Extract sections
head_section = lines[head_start + 1:equals_idx]
origin_section = lines[equals_idx + 1:origin_end + 1]

print(f"HEAD lines: {len(head_section)}")
print(f"ORIGIN lines: {len(origin_section)}")

# Find dispatch_play's Ok(()) - it should be the one just before head_start
# Search backwards from head_start
dispatch_play_ok = None
for i in range(head_start - 1, -1, -1):
    if lines[i].strip() == "Ok(())" and i + 1 < len(lines) and lines[i + 1].strip() == "}":
        dispatch_play_ok = i
        break

print(f"dispatch_play Ok(()) at: {dispatch_play_ok}")

if dispatch_play_ok is not None:
    # Build new file
    new_lines = []
    # Part 1: Up to dispatch_play's Ok(())
    new_lines.extend(lines[:dispatch_play_ok])
    # Part 2: HEAD section (indented for inside function)
    indented_head = ["    " + line if line.strip() else line for line in head_section]
    new_lines.extend([""] + indented_head + [""])
    # Part 3: From dispatch_play's Ok(()) to before conflict
    new_lines.extend(lines[dispatch_play_ok:head_start])
    # Part 4: ORIGIN section
    new_lines.extend(origin_section)
    # Part 5: After conflict
    new_lines.extend(lines[origin_end + 1:])
    
    with open("src/card_events.rs", "w") as f:
        f.write('\n'.join(new_lines))
    
    print("Fixed!")
else:
    print("Could not find dispatch_play Ok(())")