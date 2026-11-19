#!/usr/bin/env python3
"""Fix the remaining conflict in card_events.rs"""

with open("src/card_events.rs", "r") as f:
    lines = f.read().split('\n')

# The conflict:
# Line 1835 (0-indexed): <<<<<<< HEAD
# Line 1868: =======
# The HEAD section (Phase 4b) is lines 1836-1868
# The ORIGIN section (dispatch_after_worker_any_player) is lines 1869-1950
# The >>>>>>> origin/main is MISSING - should be after line 1950

# We need to:
# 1. Remove <<<<<<< HEAD at line 1835
# 2. Remove ======= at line 1868
# 3. Move Phase 4b code (lines 1836-1868) INSIDE dispatch_play before Ok(())
# 4. Keep dispatch_after_worker_any_player function (lines 1869-1950) AFTER dispatch_play
# 4. The rest of the file stays the same

# First, let's find the exact structure
# dispatch_play function ends with Ok(()) then }
# We need to insert Phase 4b before that Ok(())

# Find the dispatch_play function's Ok(())
dispatch_play_ok_idx = None
for i, line in enumerate(lines):
    if i > 1800 and line.strip() == "Ok(())" and i + 1 < len(lines) and lines[i + 1].strip() == "}":
        # Check if this is the end of dispatch_play
        # Look at context - dispatch_play is a pub fn returning Result<(), String>
        dispatch_play_ok_idx = i
        break

print(f"dispatch_play Ok(()) at line: {dispatch_play_ok_idx}")

# The Phase 4b code is between lines 1836-1868 (0-indexed: 1835-1867)
# Actually HEAD starts at 1835 (<<<<<<< HEAD), content is 1836-1868
phase_4b_start = 1836  # line after <<<<<<< HEAD
phase_4b_end = 1868    # line before =======

# The ======= is at line 1868 (0-indexed)
# The ORIGIN section starts at 1869 and goes to end of dispatch_after_worker_any_player
# which ends around line 1950 with Ok(()) and }

# Let's extract the sections
head_marker_idx = 1835  # <<<<<<< HEAD
equals_idx = 1868       # =======

head_section = lines[head_marker_idx + 1:equals_idx]  # Phase 4b code
# origin section starts after ======= and goes to end of dispatch_after_worker_any_player
# We need to find where dispatch_after_worker_any_player ends
origin_start = equals_idx + 1

# Find the end of dispatch_after_worker_any_player - it should have Ok(()) then }
origin_end = None
for i in range(origin_start, len(lines)):
    if lines[i].strip() == "Ok(())" and i + 1 < len(lines) and lines[i + 1].strip() == "}":
        # This could be the end of dispatch_after_worker_any_player
        # Check if the next line starts a new function
        if i + 2 < len(lines) and (lines[i + 2].startswith("///") or lines[i + 2].startswith("pub fn") or lines[i + 2].startswith("fn ")):
            origin_end = i + 1  # include the closing }
            break

print(f"Origin section: {origin_start} to {origin_end}")

# Now build the new file
# Structure:
# 1. Everything before <<<<<<< HEAD (line 1835)
# 2. dispatch_play with Phase 4b inserted before its Ok(())
# 3. dispatch_after_worker_any_player function (from origin section)
# 4. Everything after the origin section

# First, get the content before the conflict
before_conflict = lines[:head_marker_idx]

# Get the content after the origin section
after_origin = lines[origin_end + 1:] if origin_end else lines[equals_idx + 1:]

# Now we need to insert Phase 4b into dispatch_play
# Find dispatch_play's Ok(()) in before_conflict
dispatch_play_ok_in_before = None
for i, line in enumerate(before_conflict):
    if line.strip() == "Ok(())" and i + 1 < len(before_conflict) and before_conflict[i + 1].strip() == "}":
        dispatch_play_ok_in_before = i
        break

print(f"dispatch_play Ok(()) in before_conflict at: {dispatch_play_ok_in_before}")

# Insert Phase 4b before dispatch_play's Ok(())
if dispatch_play_ok_in_before is not None:
    # Indent Phase 4b code (it's already indented for being inside the function)
    indented_phase_4b = ["    " + line if line.strip() else line for line in head_section]
    new_before = before_conflict[:dispatch_play_ok_in_before] + [""] + indented_phase_4b + [""] + before_conflict[dispatch_play_ok_in_before:]
else:
    new_before = before_conflict

# The origin section (dispatch_after_worker_any_player) should go after dispatch_play
# Find the end of dispatch_play in new_before
dispatch_play_end = None
for i, line in enumerate(new_before):
    if i > 0 and line.strip() == "}" and new_before[i-1].strip() == "Ok(())":
        # Check if this is a top-level function end
        if i + 1 < len(new_before) and (new_before[i+1].strip() == "" or new_before[i+1].startswith("///") or new_before[i+1].startswith("pub fn") or new_before[i+1].startswith("fn ")):
            dispatch_play_end = i + 1
            break

print(f"dispatch_play end at: {dispatch_play_end}")

# Build final
if dispatch_play_end is not None and origin_end is not None:
    origin_section = lines[origin_start:origin_end + 1]
    final_lines = new_before[:dispatch_play_end] + [""] + origin_section + [""] + after_origin
else:
    final_lines = new_before + after_origin

# Write final
with open("src/card_events.rs", "w") as f:
    f.write('\n'.join(final_lines))

print("Done!")