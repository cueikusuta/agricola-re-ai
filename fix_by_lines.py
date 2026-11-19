#!/usr/bin/env python3
"""Fix remaining conflict markers by line numbers"""

with open("src/card_events.rs", "r") as f:
    lines = f.read().split('\n')

# The conflict is at line 1836 (0-indexed: 1835) with <<<<<<< HEAD
# The conflict ends at >>>>>>> origin/main which should be around line 1960
# But there's also a second conflict around line 1957

# Let's find all conflict markers
conflict_starts = []
conflict_middles = []
conflict_ends = []

for i, line in enumerate(lines):
    if '<<<<<<< HEAD' in line:
        conflict_starts.append(i)
    elif '=======' in line:
        conflict_middles.append(i)
    elif '>>>>>>> origin/main' in line:
        conflict_ends.append(i)

print(f"Conflict starts: {conflict_starts}")
print(f"Conflict middles: {conflict_middles}")
print(f"Conflict ends: {conflict_ends}")

# For each conflict, we need to resolve it
# Conflict 1: lines 1835-1951 (indices) - main conflict
# Conflict 2: lines 1956-1959 - extra blank lines

# Let's build new lines by resolving conflicts
new_lines = []
i = 0
while i < len(lines):
    if i in conflict_starts:
        start_idx = i
        # Find corresponding middle and end
        mid_idx = conflict_middles[conflict_starts.index(i)]
        end_idx = conflict_ends[conflict_starts.index(i)]
        
        print(f"Resolving conflict: start={start_idx}, mid={mid_idx}, end={end_idx}")
        
        # HEAD section: start_idx+1 to mid_idx-1
        head_section = lines[start_idx+1:mid_idx]
        # ORIGIN section: mid_idx+1 to end_idx-1
        origin_section = lines[mid_idx+1:end_idx]
        
        print(f"  HEAD lines: {len(head_section)}")
        print(f"  ORIGIN lines: {len(origin_section)}")
        
        if start_idx == 1835:  # Main conflict
            # HEAD section is Phase 4b code - should go INSIDE dispatch_play before Ok(())
            # ORIGIN section is dispatch_after_worker_any_player - should go AFTER dispatch_play
            
            # For now, we'll mark where to insert
            new_lines.append("### PHASE_4B_HERE ###")
            new_lines.extend(head_section)
            new_lines.append("### AFTER_DISPATCH_PLAY_HERE ###")
            new_lines.extend(origin_section)
        elif start_idx == 1956:  # Second conflict (extra blank lines)
            # Just skip this conflict - it's just extra blank lines
            # The correct code is already there (Ok(()) and })
            pass
        
        i = end_idx + 1
        continue
    
    new_lines.append(lines[i])
    i += 1

# Write intermediate
with open("src/card_events.rs.step1", "w") as f:
    f.write('\n'.join(new_lines))

print("Step 1 done")
print(f"Total lines: {len(new_lines)}")

# Now do step 2: insert Phase 4b inside dispatch_play and after_fn after dispatch_play
with open("src/card_events.rs.step1", "r") as f:
    lines2 = f.read().split('\n')

# Find markers
phase_4b_idx = None
after_fn_idx = None
for i, line in enumerate(lines2):
    if "### PHASE_4B_HERE ###" in line:
        phase_4b_idx = i
    elif "### AFTER_DISPATCH_PLAY_HERE ###" in line:
        after_fn_idx = i

print(f"Phase 4b marker at: {phase_4b_idx}")
print(f"After fn marker at: {after_fn_idx}")

if phase_4b_idx is not None and after_fn_idx is not None:
    # Extract sections
    phase_4b_code = lines2[phase_4b_idx + 1:after_fn_idx]
    after_fn_code = lines2[after_fn_idx + 1:]
    
    # Remove markers
    main_lines = lines2[:phase_4b_idx] + lines2[after_fn_idx + 1:]
    
    # Find dispatch_play's Ok(()) - the one at the end of the function
    insert_idx = None
    for i, line in enumerate(main_lines):
        if line.strip() == "Ok(())" and i + 1 < len(main_lines) and main_lines[i + 1].strip() == "}":
            # Check if this is the end of dispatch_play by looking at context
            # dispatch_play is a pub fn, so the closing } should be at indent 0
            if main_lines[i + 1].startswith("}") and not main_lines[i + 1].startswith("    }"):
                insert_idx = i
                break
    
    print(f"Insert index: {insert_idx}")
    if insert_idx is not None:
        # Insert Phase 4b before Ok(())
        indented_phase_4b = ["    " + line if line.strip() else line for line in phase_4b_code]
        main_lines = main_lines[:insert_idx] + [""] + indented_phase_4b + [""] + main_lines[insert_idx:]
    
    # Find end of dispatch_play to insert after_fn
    after_dispatch_idx = None
    for i, line in enumerate(main_lines):
        if i > 0 and line.strip() == "}" and main_lines[i-1].strip() == "Ok(())":
            # Check if this is a top-level function end (next line is empty or pub fn)
            if i + 1 < len(main_lines) and (main_lines[i+1].strip() == "" or main_lines[i+1].startswith("///") or main_lines[i+1].startswith("pub fn") or main_lines[i+1].startswith("fn ")):
                after_dispatch_idx = i + 1
                break
    
    print(f"After dispatch index: {after_dispatch_idx}")
    if after_dispatch_idx is not None:
        main_lines = main_lines[:after_dispatch_idx] + [""] + after_fn_code + [""] + main_lines[after_dispatch_idx:]
    
    final_lines = main_lines
else:
    final_lines = lines2

# Write final
with open("src/card_events.rs", "w") as f:
    f.write('\n'.join(final_lines))

print(f"Final lines: {len(final_lines)}")