#!/usr/bin/env python3
"""Fix the main conflict with missing >>>>>>> marker"""

with open("src/card_events.rs", "r") as f:
    lines = f.read().split('\n')

# Known line numbers:
# head_start = 1835 (<<<<<<< HEAD)
# equals_idx = 1868 (=======)
# origin_end = 1954 (end of dispatch_after_worker_any_player function, before next function)

head_start = 1835
equals_idx = 1868
origin_end = 1954  # Line with "}" before "/// Dispatch AfterWorker phase"

print(f"Conflict: head_start={head_start}, equals={equals_idx}, origin_end={origin_end}")

# Extract sections
head_section = lines[head_start + 1:equals_idx]  # Phase 4b code
origin_section = lines[equals_idx + 1:origin_end + 1]  # dispatch_after_worker_any_player function

print(f"HEAD lines: {len(head_section)}")
print(f"ORIGIN lines: {len(origin_section)}")

# Find dispatch_play's Ok(()) - the one at the end of dispatch_play function
dispatch_play_ok = None
for i, line in enumerate(lines):
    if i < head_start and line.strip() == "Ok(())" and i + 1 < len(lines) and lines[i + 1].strip() == "}":
        dispatch_play_ok = i
        break

print(f"dispatch_play Ok(()) at: {dispatch_play_ok}")

if dispatch_play_ok is not None:
    # Build new file:
    # 1. Lines up to dispatch_play_ok (inside dispatch_play)
    # 2. HEAD section (indented) - Phase 4b
    # 3. Lines from dispatch_play_ok to head_start (rest of dispatch_play)
    # 4. ORIGIN section (dispatch_after_worker_any_player)
    # 5. Lines after origin_end
    
    new_lines = []
    # Part 1: Up to dispatch_play's Ok(())
    new_lines.extend(lines[:dispatch_play_ok])
    # Part 2: HEAD section (indented for inside function)
    indented_head = ["    " + line if line.strip() else line for line in head_section]
    new_lines.extend([""] + indented_head + [""])
    # Part 3: From dispatch_play's Ok(()) to before conflict
    new_lines.extend(lines[dispatch_play_ok:head_start])
    # Part 4: ORIGIN section (dispatch_after_worker_any_player)
    new_lines.extend(origin_section)
    # Part 5: After conflict (from origin_end + 1)
    new_lines.extend(lines[origin_end + 1:])
    
    with open("src/card_events.rs", "w") as f:
        f.write('\n'.join(new_lines))
    
    print("Fixed!")
else:
    print("Could not find dispatch_play Ok(())")