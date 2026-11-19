#!/usr/bin/env python3
"""Move Phase 4b code inside dispatch_play"""

with open("src/card_events.rs", "r") as f:
    lines = f.read().split('\n')

# Find the Phase 4b code block (starts with "// Phase 4b: After — improvement-play dependent triggers")
phase_4b_start = None
phase_4b_end = None

for i, line in enumerate(lines):
    if "Phase 4b: After — improvement-play dependent triggers" in line:
        phase_4b_start = i
        break

if phase_4b_start is None:
    print("Phase 4b not found!")
    exit(1)

# Find the end of Phase 4b - it ends before "/// Dispatch AfterAnyWorkerGain"
for i in range(phase_4b_start, len(lines)):
    if "/// Dispatch AfterAnyWorkerGain" in lines[i]:
        phase_4b_end = i
        break

print(f"Phase 4b: lines {phase_4b_start} to {phase_4b_end}")

# Extract Phase 4b code
phase_4b_code = lines[phase_4b_start:phase_4b_end]

# Find dispatch_play's Ok(()) - the one at the end of dispatch_play function
dispatch_play_ok = None
for i, line in enumerate(lines):
    if i > 1800 and line.strip() == "Ok(())" and i + 1 < len(lines) and lines[i + 1].strip() == "}":
        # Check if this is dispatch_play (not a nested function)
        # dispatch_play is a pub fn, so the closing } should be at indent 0
        if lines[i + 1].strip() == "}":
            dispatch_play_ok = i
            break

print(f"dispatch_play Ok(()) at: {dispatch_play_ok}")

if dispatch_play_ok is not None and phase_4b_start is not None:
    # Remove Phase 4b from its current location (after dispatch_play)
    new_lines = lines[:phase_4b_start] + lines[phase_4b_end:]
    
    # Adjust dispatch_play_ok if it shifted
    if dispatch_play_ok > phase_4b_start:
        dispatch_play_ok -= (phase_4b_end - phase_4b_start)
    
    # Insert Phase 4b before dispatch_play's Ok(())
    indented_phase_4b = ["    " + line if line.strip() else line for line in phase_4b_code]
    final_lines = new_lines[:dispatch_play_ok] + [""] + indented_phase_4b + [""] + new_lines[dispatch_play_ok:]
    
    with open("src/card_events.rs", "w") as f:
        f.write('\n'.join(final_lines))
    
    print("Phase 4b moved inside dispatch_play!")
else:
    print("Could not find required locations")