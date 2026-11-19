#!/usr/bin/env python3
"""Fix conflict markers in card_events.rs properly"""

with open("src/card_events.rs", "r") as f:
    content = f.read()

lines = content.split('\n')

new_lines = []
i = 0
while i < len(lines):
    line = lines[i]
    
    # Conflict 1: <<<<<<< HEAD at line 1836 (0-indexed: 1835)
    if i == 1835 and '<<<<<<< HEAD' in line:
        i += 1  # skip HEAD marker
        # Collect HEAD section (Phase 4b code)
        head_section = []
        while i < len(lines) and '=======' not in lines[i]:
            head_section.append(lines[i])
            i += 1
        i += 1  # skip =======
        # Collect ORIGIN section (dispatch_after_worker_any_player function)
        origin_section = []
        while i < len(lines) and '>>>>>>> origin/main' not in lines[i]:
            origin_section.append(lines[i])
            i += 1
        i += 1  # skip >>>>>>>
        
        # The head_section is the Phase 4b code that should go INSIDE dispatch_play
        # The origin_section is the dispatch_after_worker_any_player function that should go AFTER dispatch_play
        
        # We need to insert the Phase 4b code before the Ok(()) in dispatch_play
        # The dispatch_play function should end with the Phase 4b code then Ok(())
        # Then the origin_section function should follow
        
        # For now, let's add a marker and we'll do a second pass
        new_lines.append("### INSERT_PHASE_4B_HERE ###")
        new_lines.extend(head_section)
        new_lines.append("### INSERT_AFTER_DISPATCH_PLAY ###")
        new_lines.extend(origin_section)
        continue
    
    # Conflict 2: lines 1957-1960 (0-indexed: 1956-1959) - extra blank lines
    if i == 1956 and '<<<<<<< HEAD' in line:
        # Skip HEAD marker
        i += 1
        # Skip until =======
        while i < len(lines) and '=======' not in lines[i]:
            i += 1
        i += 1  # skip =======
        # Skip until >>>>>>>
        while i < len(lines) and '>>>>>>> origin/main' not in lines[i]:
            i += 1
        i += 1  # skip >>>>>>>
        continue
    
    new_lines.append(line)
    i += 1

# Now do a second pass to place the Phase 4b code inside dispatch_play
# Find the dispatch_play function end (Ok(())) and insert before it
final_lines = []
i = 0
while i < len(new_lines):
    line = new_lines[i]
    
    if line == "### INSERT_PHASE_4B_HERE ###":
        # We're at the position where the conflict was - after dispatch_play ended
        # We need to go back and insert the Phase 4b code before Ok(()) in dispatch_play
        # But since we're doing a linear pass, let's collect the sections
        i += 1
        # Collect phase 4b code
        phase_4b = []
        while i < len(new_lines) and new_lines[i] != "### INSERT_AFTER_DISPATCH_PLAY ###":
            phase_4b.append(new_lines[i])
            i += 1
        i += 1  # skip marker
        # Collect after function
        after_fn = []
        while i < len(new_lines) and new_lines[i] != "### END_CONFLICT_1 ###":
            after_fn.append(new_lines[i])
            i += 1
        if i < len(new_lines):
            i += 1  # skip end marker
        
        # Store for later insertion
        final_lines.append("### PHASE_4B_CODE ###")
        final_lines.extend(phase_4b)
        final_lines.append("### AFTER_DISPATCH_PLAY_FN ###")
        final_lines.extend(after_fn)
        continue
    
    final_lines.append(line)
    i += 1

# Write intermediate
with open("src/card_events.rs.pass1", "w") as f:
    f.write('\n'.join(final_lines))

print(f"Pass 1 done: {len(final_lines)} lines")