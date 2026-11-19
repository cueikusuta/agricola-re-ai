#!/usr/bin/env python3
"""Fix conflict markers in card_events.rs"""

with open("src/card_events.rs", "r") as f:
    content = f.read()

lines = content.split('\n')

# Fix conflict 1: lines 1836-1952 (Phase 4b vs dispatch_after_worker_any_player)
# Fix conflict 2: lines 1957-1960 (extra blank lines)

# We'll rebuild the file by processing the conflicts
new_lines = []
i = 0
while i < len(lines):
    line = lines[i]
    
    # Conflict 1: <<<<<<< HEAD at line 1836 (0-indexed: 1835)
    if i == 1835 and '<<<<<<< HEAD' in line:
        # Skip HEAD marker
        i += 1
        # Collect HEAD section until =======
        head_section = []
        while i < len(lines) and '=======' not in lines[i]:
            head_section.append(lines[i])
            i += 1
        # Skip =======
        i += 1
        # Collect ORIGIN section until >>>>>>>
        origin_section = []
        while i < len(lines) and '>>>>>>> origin/main' not in lines[i]:
            origin_section.append(lines[i])
            i += 1
        # Skip >>>>>>>
        i += 1
        
        # Now we need to:
        # 1. Insert the Phase 4b code (head_section) INSIDE dispatch_play, before Ok(())
        # 2. Keep the dispatch_after_worker_any_player function (origin_section) AFTER dispatch_play
        
        # But the current structure has the dispatch_play function ending at line ~1834
        # The head_section is the Phase 4b code that should go inside
        # The origin_section is the dispatch_after_worker_any_player function
        
        # We'll handle this by marking where to insert
        # For now, let's just collect what we need
        print(f"Conflict 1: HEAD lines={len(head_section)}, ORIGIN lines={len(origin_section)}")
        
        # We'll insert the Phase 4b code before the closing of dispatch_play
        # And add the origin function after dispatch_play
        # This is complex - let's just reconstruct the file properly
        
        # Mark that we need to fix this region
        new_lines.append("### CONFLICT_1_REGION ###")
        new_lines.extend(head_section)
        new_lines.append("### MIDDLE ###")
        new_lines.extend(origin_section)
        new_lines.append("### END_CONFLICT_1 ###")
        continue
    
    # Conflict 2: lines 1957-1960 (0-indexed: 1956-1959)
    if i == 1956 and '<<<<<<< HEAD' in line:
        # Skip to after >>>>>>>
        while i < len(lines) and '>>>>>>> origin/main' not in lines[i]:
            i += 1
        i += 1  # skip >>>>>>>
        continue
    
    new_lines.append(line)
    i += 1

# Write intermediate result for inspection
with open("src/card_events.rs.fixed1", "w") as f:
    f.write('\n'.join(new_lines))

print("Done first pass")
print(f"Total lines: {len(new_lines)}")