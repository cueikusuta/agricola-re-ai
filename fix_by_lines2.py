#!/usr/bin/env python3
"""Fix the main conflict by line numbers"""

with open("src/card_events.rs", "r") as f:
    lines = f.read().split('\n')

# Find the conflict markers
head_start = None
equals_idx = None
origin_end = None

for i, line in enumerate(lines):
    if '<<<<<<< HEAD' in line and head_start is None:
        head_start = i
    elif '=======' in line and equals_idx is None and head_start is not None:
        equals_idx = i
    elif '>>>>>>> origin/main' in line and origin_end is None and equals_idx is not None:
        origin_end = i
        break

print(f"Conflict: head_start={head_start}, equals={equals_idx}, origin_end={origin_end}")

if head_start is not None and equals_idx is not None and origin_end is not None:
    # HEAD section: head_start+1 to equals_idx-1
    head_section = lines[head_start + 1:equals_idx]
    # ORIGIN section: equals_idx+1 to origin_end-1
    origin_section = lines[equals_idx + 1:origin_end]
    
    print(f"HEAD lines: {len(head_section)}")
    print(f"ORIGIN lines: {len(origin_section)}")
    
    # Build new lines:
    # 1. Lines before HEAD marker
    # 2. HEAD section (Phase 4b) - but indented for inside dispatch_play
    # 3. Lines between HEAD and ORIGIN (should be empty or just the =======)
    # 4. ORIGIN section (dispatch_after_worker_any_player function)
    # 5. Lines after origin_end
    
    new_lines = []
    new_lines.extend(lines[:head_start])  # Before conflict
    
    # Add HEAD section (Phase 4b) - but we need to insert it INSIDE dispatch_play
    # For now, add it here (after dispatch_play ends)
    # Actually, the HEAD section should go INSIDE dispatch_play before Ok(())
    # But the current structure has the conflict AFTER dispatch_play ends
    # So we need to move the HEAD section to inside dispatch_play
    
    # For now, let's just resolve the conflict by keeping both sections properly
    # The HEAD section goes inside dispatch_play (we'll handle that separately)
    # The ORIGIN section goes after dispatch_play
    
    # Since the conflict is AFTER dispatch_play ends, we need to:
    # 1. Remove the conflict markers
    # 2. Put ORIGIN section after dispatch_play
    # 3. Move HEAD section inside dispatch_play (before its Ok(()))
    
    # But the current structure in the file has the conflict markers where dispatch_play already ended
    # So we need to:
    # - Remove lines[head_start:origin_end+1] (the entire conflict)
    # - Insert ORIGIN section after dispatch_play
    # - Insert HEAD section inside dispatch_play before its Ok(())
    
    # Find dispatch_play's Ok(())
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
        # Part 4: ORIGIN section
        new_lines.extend(origin_section)
        # Part 5: After conflict
        new_lines.extend(lines[origin_end + 1:])
        
        with open("src/card_events.rs", "w") as f:
            f.write('\n'.join(new_lines))
        
        print("Fixed!")
    else:
        print("Could not find dispatch_play Ok(())")
else:
    print("Conflict markers not found properly")