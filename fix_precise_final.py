#!/usr/bin/env python3
"""Fix remaining conflict markers by exact line removal"""

with open("src/card_events.rs", "r") as f:
    lines = f.read().split('\n')

# Find all conflict markers
head_start = None
equals_idx = None

for i, line in enumerate(lines):
    if '<<<<<<< HEAD' in line and head_start is None:
        head_start = i
    elif '=======' in line and equals_idx is None and head_start is not None:
        equals_idx = i

print(f"Conflict: head_start={head_start}, equals={equals_idx}")

if head_start is not None and equals_idx is not None:
    # Find the end of the origin section - it's the dispatch_after_worker_any_player function
    # which ends before "/// Dispatch AfterWorker phase"
    origin_end = None
    for i in range(equals_idx + 1, len(lines)):
        if "/// Dispatch AfterWorker phase for a completed worker action" in lines[i]:
            origin_end = i - 1  # Line before this comment
            break
    
    if origin_end is None:
        # Fallback: find the Ok(()) } that ends dispatch_after_worker_any_player
        for i in range(equals_idx + 1, len(lines)):
            if lines[i].strip() == "Ok(())" and i + 1 < len(lines) and lines[i + 1].strip() == "}":
                # Check if next line starts a new function
                if i + 2 < len(lines) and lines[i + 2].startswith("///"):
                    origin_end = i + 1
                    break
    
    print(f"origin_end: {origin_end}")
    
    if origin_end is not None:
        # Find dispatch_play's Ok(()) - search backwards from head_start
        dispatch_play_ok = None
        for i in range(head_start - 1, -1, -1):
            if lines[i].strip() == "Ok(())" and i + 1 < len(lines) and lines[i + 1].strip() == "}":
                dispatch_play_ok = i
                break
        
        print(f"dispatch_play Ok(()) at: {dispatch_play_ok}")
        
        if dispatch_play_ok is not None:
            # Extract sections
            head_section = lines[head_start + 1:equals_idx]
            origin_section = lines[equals_idx + 1:origin_end + 1]
            
            # Build new file
            new_lines = []
            # 1. Up to dispatch_play's Ok(())
            new_lines.extend(lines[:dispatch_play_ok])
            # 2. HEAD section (indented) - Phase 4b
            indented_head = ["    " + line if line.strip() else line for line in lines[head_start + 1:equals_idx]]
            new_lines.extend([""] + indented_head + [""])
            # 3. From dispatch_play's Ok(()) to before conflict
            new_lines.extend(lines[dispatch_play_ok:head_start])
            # 4. ORIGIN section (dispatch_after_worker_any_player)
            new_lines.extend(lines[equals_idx + 1:origin_end + 1])
            # 5. After conflict
            new_lines.extend(lines[origin_end + 1:])
            
            with open("src/card_events.rs", "w") as f:
                f.write('\n'.join(new_lines))
            
            print("Fixed!")
        else:
            print("Could not find dispatch_play Ok(())")
    else:
        print("Could not find origin_end")
else:
    print("Conflict markers not found")

# Also check for any remaining conflict markers
with open("src/card_events.rs", "r") as f:
    content = f.read()

remaining = content.count('<<<<<<< HEAD') + content.count('=======') + content.count('>>>>>>> origin/main')
print(f"Remaining conflict markers: {remaining}")